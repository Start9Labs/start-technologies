use super::*;
use crate::registry::package::icon::store_icon;
use crate::sign::AnySignature;

struct Fixture {
    dir: tempfile::TempDir,
    main: TypedPatchDb<RegistryDatabase>,
    public_icon: Value,
}

fn wire(version: &str) -> WireCompatibility {
    WireCompatibility::for_startos(&version.parse().unwrap()).unwrap()
}

fn store(dir: &Path, data: &str) -> Value {
    to_value(&store_icon(&icons_dir(dir), &data.parse().unwrap()).unwrap()).unwrap()
}

async fn fixture() -> Fixture {
    use ed25519_dalek::Signer;

    let dir = tempfile::tempdir().unwrap();
    let public_icon = store(dir.path(), "data:image/png;base64,cHVibGlj");
    let private_icon = store(dir.path(), "data:image/png;base64,c2VjcmV0");
    let reader = Guid::new();
    let mut state = root(true, &reader, &[]);
    let signature = ed25519_dalek::SigningKey::from_bytes(&[7; 32]).sign(b"asset");
    for (name, icon) in [("public", &public_icon), ("secret", &private_icon)] {
        let info = &mut state["index"]["package"]["packages"][name]["versions"]["1.0.0:0"];
        info["icon"] = icon.clone();
        info["dependencyMetadata"] = json!({ "dep": { "icon": icon } });
        info["s9pks"] = json!([[{}, { "signatures": {
            key().interned_der64().to_string(): AnySignature::Ed25519ph(signature),
        } }]]);
    }
    state["index"]["os"]["versions"] = json!({ "0.4.0.6": {
        "releaseNotes": { "en_US": "English", "pl_PL": "Polski" },
    } });
    let main = TypedPatchDb::<RegistryDatabase>::load_unchecked(PatchDb::in_memory());
    main.put(&JsonPointer::<&str>::default(), &state)
        .await
        .unwrap();
    Fixture {
        dir,
        main,
        public_icon,
    }
}

#[tokio::test]
async fn equivalent_versions_share_replicas_and_versioned_privacy_survives_revisions() {
    let fixture = fixture().await;
    let mirrors = IndexMirrors::default();
    let old = index_mirror(
        &mirrors,
        &fixture.main,
        None,
        wire("0.4.0.2"),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    let same = index_mirror(
        &mirrors,
        &fixture.main,
        None,
        wire("0.4.0.1"),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    assert!(Arc::ptr_eq(&old, &same));
    let current = index_mirror(
        &mirrors,
        &fixture.main,
        None,
        wire("0.4.1.3"),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    let unversioned = index_mirror(
        &mirrors,
        &fixture.main,
        None,
        WireCompatibility::default(),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    assert!(Arc::ptr_eq(&current, &unversioned));
    assert!(!Arc::ptr_eq(&old, &current));
    let own = index_mirror(
        &mirrors,
        &fixture.main,
        Some(key()),
        wire("0.4.0.2"),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    assert!(!Arc::ptr_eq(&old, &own));
    let middle = index_mirror(
        &mirrors,
        &fixture.main,
        None,
        wire("0.4.0.4"),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    assert!(!Arc::ptr_eq(&old, &middle));
    assert!(!Arc::ptr_eq(&current, &middle));
    assert_eq!(mirrors.lock().await.len(), 4);

    let pointer: JsonPointer = "/index".parse().unwrap();
    let (old_dump, mut old_sub) = old.db.dump_and_sub(pointer.clone()).await;
    let (new_dump, mut new_sub) = current.db.dump_and_sub(pointer.clone()).await;
    let (own_dump, mut own_sub) = own.db.dump_and_sub(pointer.clone()).await;
    let mut old_view = old_dump.value;
    let mut new_view = new_dump.value;
    let mut own_view = own_dump.value;
    let middle_view = middle.db.dump(&pointer).await.value;
    assert_eq!(
        old_view["os"]["versions"]["0.4.0.6"]["releaseNotes"],
        json!("English")
    );
    assert!(new_view["os"]["versions"]["0.4.0.6"]["releaseNotes"].is_object());
    assert_eq!(
        middle_view["os"]["versions"]["0.4.0.6"]["releaseNotes"],
        json!("English")
    );
    let public = &old_view["package"]["packages"]["public"]["versions"]["1.0.0:0"];
    assert_eq!(public["icon"], json!("data:image/png;base64,cHVibGlj"));
    assert_eq!(public["dependencyMetadata"]["dep"]["icon"], public["icon"]);
    assert_eq!(
        new_view["package"]["packages"]["public"]["versions"]["1.0.0:0"]["icon"],
        fixture.public_icon
    );
    assert_eq!(
        middle_view["package"]["packages"]["public"]["versions"]["1.0.0:0"]["icon"],
        fixture.public_icon
    );
    assert_eq!(
        old_view["signers"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap()["keys"][0],
        json!(key().to_string())
    );
    let signature: AnySignature =
        from_value(public["s9pks"][0][1]["signatures"][&key().to_string()].clone()).unwrap();
    assert!(matches!(signature, AnySignature::Ed25519(_)));
    assert!(old_view["package"]["packages"].get("secret").is_none());
    assert!(!old_view.to_string().contains("c2VjcmV0"));
    assert_eq!(
        own_view["package"]["packages"]["secret"]["versions"]["1.0.0:0"]["icon"],
        json!("data:image/png;base64,c2VjcmV0")
    );
    assert_eq!(
        own_view["package"]["packages"]["secret"]["readers"],
        json!([])
    );

    let mut state = fixture.main.peek().await.into_value();
    state["index"]["os"]["versions"]["0.4.0.6"]["releaseNotes"] =
        json!({ "en_US": "Updated", "pl_PL": "Nowe" });
    state["index"]["package"]["packages"]["secret"]["readers"] = json!([]);
    fixture
        .main
        .put(&JsonPointer::<&str>::default(), &state)
        .await
        .unwrap();
    for (sub, view) in [
        (&mut old_sub, &mut old_view),
        (&mut new_sub, &mut new_view),
        (&mut own_sub, &mut own_view),
    ] {
        let revision = tokio::time::timeout(std::time::Duration::from_secs(5), sub.recv())
            .await
            .unwrap()
            .unwrap();
        patch_db::json_patch::patch(view, &revision.patch).unwrap();
        assert!(view["package"]["packages"].get("secret").is_none());
    }
    assert_eq!(
        old_view["os"]["versions"]["0.4.0.6"]["releaseNotes"],
        json!("Updated")
    );
    assert_eq!(
        own_view["os"]["versions"]["0.4.0.6"]["releaseNotes"],
        json!("Updated")
    );
    assert_eq!(
        new_view["os"]["versions"]["0.4.0.6"]["releaseNotes"],
        json!({ "en_US": "Updated", "pl_PL": "Nowe" })
    );
}

#[tokio::test]
async fn inaccessible_private_icons_are_not_loaded_by_legacy_replicas() {
    let fixture = fixture().await;
    let mut state = fixture.main.peek().await.into_value();
    let secret = &mut state["index"]["package"]["packages"]["secret"]["versions"]["1.0.0:0"];
    let private = secret["icon"].as_str().unwrap();
    std::fs::remove_file(icons_dir(fixture.dir.path()).join(private)).unwrap();
    fixture
        .main
        .put(&JsonPointer::<&str>::default(), &state)
        .await
        .unwrap();
    let replica = index_mirror(
        &IndexMirrors::default(),
        &fixture.main,
        None,
        wire("0.4.0.2"),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    assert_eq!(
        packages(&replica.db.dump(&JsonPointer::<&str>::default()).await.value),
        ["public"]
    );
}

#[tokio::test]
async fn collected_icon_in_an_older_revision_terminates_and_rebuilds_the_replica() {
    let fixture = fixture().await;
    let mirrors = IndexMirrors::default();
    let old = index_mirror(
        &mirrors,
        &fixture.main,
        None,
        wire("0.4.0.2"),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    let current = index_mirror(
        &mirrors,
        &fixture.main,
        None,
        WireCompatibility::default(),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    let next_icon = store(fixture.dir.path(), "data:image/png;base64,bmV4dA==");
    let mut state = fixture.main.peek().await.into_value();
    state["index"]["name"] = json!("Renamed");
    fixture
        .main
        .put(&JsonPointer::<&str>::default(), &state)
        .await
        .unwrap();
    let public = &mut state["index"]["package"]["packages"]["public"]["versions"]["1.0.0:0"];
    public["icon"] = next_icon.clone();
    public["dependencyMetadata"]["dep"]["icon"] = next_icon;
    fixture
        .main
        .put(&JsonPointer::<&str>::default(), &state)
        .await
        .unwrap();
    std::fs::remove_file(icons_dir(fixture.dir.path()).join(fixture.public_icon.as_str().unwrap()))
        .unwrap();
    let failure = tokio::time::timeout(std::time::Duration::from_secs(5), old.closed())
        .await
        .unwrap();
    assert!(failure.is_err());
    assert!(!old.is_running());
    assert!(current.is_running());
    let fresh = index_mirror(
        &mirrors,
        &fixture.main,
        None,
        wire("0.4.0.2"),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    assert!(!Arc::ptr_eq(&old, &fresh));
    assert!(fresh.is_running());
    let view = fresh.db.dump(&JsonPointer::<&str>::default()).await.value;
    assert_eq!(
        view["index"]["package"]["packages"]["public"]["versions"]["1.0.0:0"]["icon"],
        json!("data:image/png;base64,bmV4dA==")
    );
}
