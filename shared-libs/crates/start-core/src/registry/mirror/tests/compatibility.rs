use super::*;
use crate::registry::locales::IndexLocales;
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
        "headline": "Stored unused headline",
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
        wire("0.4.0.2").into(),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    let same = index_mirror(
        &mirrors,
        &fixture.main,
        None,
        wire("0.4.0.1").into(),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    assert!(Arc::ptr_eq(&old, &same));
    let current = index_mirror(
        &mirrors,
        &fixture.main,
        None,
        wire("0.4.3.1").into(),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    let unversioned = index_mirror(
        &mirrors,
        &fixture.main,
        None,
        IndexView::default(),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    assert!(Arc::ptr_eq(&current, &unversioned));
    assert!(!Arc::ptr_eq(&old, &current));
    let headline = index_mirror(
        &mirrors,
        &fixture.main,
        None,
        wire("0.4.3").into(),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    assert!(!Arc::ptr_eq(&headline, &current));
    let same_headline = index_mirror(
        &mirrors,
        &fixture.main,
        None,
        wire("0.4.1.3").into(),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    assert!(Arc::ptr_eq(&headline, &same_headline));
    let own = index_mirror(
        &mirrors,
        &fixture.main,
        Some(key()),
        wire("0.4.0.2").into(),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    assert!(!Arc::ptr_eq(&old, &own));
    let middle = index_mirror(
        &mirrors,
        &fixture.main,
        None,
        wire("0.4.0.4").into(),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    assert!(!Arc::ptr_eq(&old, &middle));
    assert!(!Arc::ptr_eq(&current, &middle));
    assert_eq!(mirrors.lock().await.len(), 5);

    let pointer: JsonPointer = "/index".parse().unwrap();
    let (old_dump, mut old_sub) = old.db.dump_and_sub(pointer.clone()).await;
    let (new_dump, mut new_sub) = current.db.dump_and_sub(pointer.clone()).await;
    let (own_dump, mut own_sub) = own.db.dump_and_sub(pointer.clone()).await;
    let mut old_view = old_dump.value;
    let mut new_view = new_dump.value;
    let mut own_view = own_dump.value;
    let middle_view = middle.db.dump(&pointer).await.value;
    let headline_view = headline.db.dump(&pointer).await.value;
    assert_eq!(
        headline_view["os"]["versions"]["0.4.0.6"]["headline"],
        json!("v0.4.0.6")
    );
    assert!(headline_view["os"]["versions"]["0.4.0.6"]["releaseNotes"].is_object());
    assert_eq!(
        old_view["os"]["versions"]["0.4.0.6"]["headline"],
        json!("v0.4.0.6")
    );
    assert!(
        new_view["os"]["versions"]["0.4.0.6"]
            .get("headline")
            .is_none()
    );
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
    assert_eq!(
        old_view["os"]["versions"]["0.4.0.6"]["headline"],
        json!("v0.4.0.6")
    );
    assert!(
        new_view["os"]["versions"]["0.4.0.6"]
            .get("headline")
            .is_none()
    );
    assert_eq!(
        fixture.main.peek().await.into_value()["index"]["os"]["versions"]["0.4.0.6"]["headline"],
        json!("Stored unused headline")
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
        wire("0.4.0.2").into(),
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
        wire("0.4.0.2").into(),
        fixture.dir.path(),
    )
    .await
    .unwrap();
    let current = index_mirror(
        &mirrors,
        &fixture.main,
        None,
        IndexView::default(),
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
        wire("0.4.0.2").into(),
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

fn translated(en: &str, pl: &str) -> Value {
    json!({ "en_US": en, "pl_PL": pl })
}

#[tokio::test]
async fn language_views_share_replicas_and_localize_snapshots_and_revisions() {
    let fixture = fixture().await;
    let mut state = fixture.main.peek().await.into_value();
    state["index"]["description"] = translated("Registry", "Rejestr");
    state["index"]["defaultLocale"] = json!("pl_PL");
    state["index"]["package"]["categories"] =
        json!({ "tools": { "name": translated("Tools", "Narzędzia") } });
    let public = &mut state["index"]["package"]["packages"]["public"]["versions"]["1.0.0:0"];
    public["description"] = json!({ "short": translated("Short", "Krótki"), "long": "Long" });
    public["releaseNotes"] = translated("Notes", "Notatki");
    public["defaultLocale"] = json!("en_US");
    public["preDownloadAlert"] = json!({
        "message": translated("Back up", "Zrób kopię"),
        "when": { "sourceVersion": "*" },
    });
    public["dependencyMetadata"]["dep"]["title"] = translated("Dependency", "Zależność");
    public["dependencyMetadata"]["dep"]["description"] = json!(null);
    public["dependencyMetadata"]["dep"]["optional"] = json!(false);
    let secret = &mut state["index"]["package"]["packages"]["secret"]["versions"]["1.0.0:0"];
    secret["description"] = json!({ "short": "Short", "long": "Long" });
    secret["releaseNotes"] = json!({ "en_US": "Notes", "ja_JP": "ノート" });
    secret["dependencyMetadata"]["dep"]["title"] = json!(null);
    secret["dependencyMetadata"]["dep"]["description"] = json!(null);
    secret["dependencyMetadata"]["dep"]["optional"] = json!(false);
    fixture
        .main
        .put(&JsonPointer::<&str>::default(), &state)
        .await
        .unwrap();

    let snapshot = fixture.main.peek().await;
    let locales = IndexLocales::of(snapshot.as_index()).unwrap();
    let mirrors = IndexMirrors::default();
    let reader_mirror =
        |key: Option<AnyVerifyingKey>, wire: WireCompatibility, language: Option<&str>| {
            let view = match language {
                Some(language) => {
                    IndexView::localized(&snapshot, &locales, key.as_ref(), wire, language).unwrap()
                }
                None => IndexView::from(wire),
            };
            index_mirror(&mirrors, &fixture.main, key, view, fixture.dir.path())
        };
    let mirror =
        |wire: WireCompatibility, language: Option<&str>| reader_mirror(None, wire, language);
    let current = WireCompatibility::default();
    let polish = mirror(current, Some("pl_PL")).await.unwrap();
    for related in ["pl_PL", "pl", "pl_XX"] {
        assert!(Arc::ptr_eq(
            &polish,
            &mirror(current, Some(related)).await.unwrap()
        ));
    }
    let canonical = mirror(current, None).await.unwrap();
    let english = mirror(current, Some("en_US")).await.unwrap();
    assert!(Arc::ptr_eq(
        &english,
        &mirror(current, Some("en_GB")).await.unwrap()
    ));
    let default_view = mirror(current, Some("zz_ZZ")).await.unwrap();
    assert!(!Arc::ptr_eq(&english, &default_view));
    for absent in ["", "de_DE", "ja_JP"] {
        assert!(Arc::ptr_eq(
            &default_view,
            &mirror(current, Some(absent)).await.unwrap()
        ));
    }
    let japanese = reader_mirror(Some(key()), current, Some("ja_JP"))
        .await
        .unwrap();
    let reader_english = reader_mirror(Some(key()), current, Some("en_US"))
        .await
        .unwrap();
    assert!(!Arc::ptr_eq(&japanese, &reader_english));
    assert_eq!(
        japanese.db.dump(&"/index".parse().unwrap()).await.value["package"]["packages"]["secret"]["versions"]
            ["1.0.0:0"]["releaseNotes"],
        json!("ノート")
    );
    drop((japanese, reader_english));
    let legacy_polish = mirror(wire("0.4.0.2"), Some("pl_PL")).await.unwrap();
    for other in [&canonical, &english, &default_view, &legacy_polish] {
        assert!(!Arc::ptr_eq(&polish, other));
    }
    assert_eq!(mirrors.lock().await.len(), 5);

    let pointer: JsonPointer = "/index".parse().unwrap();
    let (dump, mut sub) = polish.db.dump_and_sub(pointer.clone()).await;
    let mut polish_view = dump.value;
    assert_eq!(polish_view["description"], json!("Rejestr"));
    assert_eq!(
        polish_view["package"]["categories"]["tools"]["name"],
        json!("Narzędzia")
    );
    let version = &polish_view["package"]["packages"]["public"]["versions"]["1.0.0:0"];
    assert_eq!(version["description"]["short"], json!("Krótki"));
    assert_eq!(version["description"]["long"], json!("Long"));
    assert_eq!(version["releaseNotes"], json!("Notatki"));
    assert_eq!(version["preDownloadAlert"]["message"], json!("Zrób kopię"));
    assert_eq!(
        version["dependencyMetadata"]["dep"]["title"],
        json!("Zależność")
    );
    assert_eq!(
        polish_view["os"]["versions"]["0.4.0.6"]["releaseNotes"],
        json!("Polski")
    );
    let english_view = english.db.dump(&pointer).await.value;
    assert_eq!(english_view["description"], json!("Registry"));
    assert_eq!(
        english_view["os"]["versions"]["0.4.0.6"]["releaseNotes"],
        json!("English")
    );
    assert_eq!(
        english_view["package"]["packages"]["public"]["versions"]["1.0.0:0"]["releaseNotes"],
        json!("Notes")
    );
    let default_view = default_view.db.dump(&pointer).await.value;
    assert_eq!(default_view["description"], json!("Rejestr"));
    assert_eq!(
        default_view["package"]["categories"]["tools"]["name"],
        json!("Narzędzia")
    );
    assert_eq!(
        default_view["package"]["packages"]["public"]["versions"]["1.0.0:0"]["releaseNotes"],
        json!("Notes")
    );
    let legacy_view = legacy_polish.db.dump(&pointer).await.value;
    assert_eq!(
        legacy_view["os"]["versions"]["0.4.0.6"]["releaseNotes"],
        json!("Polski")
    );
    assert_eq!(
        legacy_view["package"]["packages"]["public"]["versions"]["1.0.0:0"]["icon"],
        json!("data:image/png;base64,cHVibGlj")
    );
    let canonical_view = canonical.db.dump(&pointer).await.value;
    assert_eq!(
        canonical_view["description"],
        translated("Registry", "Rejestr")
    );

    state["index"]["os"]["versions"]["0.4.0.6"]["releaseNotes"] = translated("Updated", "Nowe");
    fixture
        .main
        .put(&JsonPointer::<&str>::default(), &state)
        .await
        .unwrap();
    let revision = tokio::time::timeout(std::time::Duration::from_secs(5), sub.recv())
        .await
        .unwrap()
        .unwrap();
    patch_db::json_patch::patch(&mut polish_view, &revision.patch).unwrap();
    assert_eq!(
        polish_view["os"]["versions"]["0.4.0.6"]["releaseNotes"],
        json!("Nowe")
    );
    assert_eq!(
        fixture.main.peek().await.into_value()["index"]["description"],
        translated("Registry", "Rejestr")
    );
}
