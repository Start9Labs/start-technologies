use super::*;
use crate::sign::AnySigningKey;

fn key() -> PeerIdentity {
    let AnySigningKey::Ed25519(key) = AnySigningKey::generate();
    PeerIdentity {
        key: AnyVerifyingKey::Ed25519(key.verifying_key()),
        temporary: false,
    }
}

#[tokio::test]
async fn first_contact_is_silent_and_rotations_are_remembered_without_a_prompt() {
    let temp = tempfile::tempdir().unwrap();
    let url: Url = "https://registry.test/catalog".parse().unwrap();
    let first = key();
    let second = key();
    assert!(
        observe(temp.path(), &url, "registry.test", &first)
            .await
            .is_empty()
    );
    assert!(
        observe(temp.path(), &url, "registry.test", &first)
            .await
            .is_empty()
    );
    let warnings = observe(temp.path(), &url, "registry.test", &second).await;
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains(url.as_str()));
    assert!(warnings[0].contains(&fingerprint(&first.key)));
    assert!(warnings[0].contains(&fingerprint(&second.key)));
    assert!(warnings[0].contains("trusted channel"));
    assert_eq!(
        crate::util::io::read_file_to_string(identity_file(temp.path(), &url))
            .await
            .unwrap()
            .parse::<AnyVerifyingKey>()
            .unwrap(),
        second.key
    );
    assert!(
        observe(temp.path(), &url, "registry.test", &second)
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn equivalent_registry_bases_share_rotation_history() {
    for (first_url, second_url) in [
        (
            "https://registry.test/catalog",
            "https://registry.test/catalog/",
        ),
        (
            "https://registry.test/catalog/",
            "https://registry.test/catalog",
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let first_url: Url = first_url.parse().unwrap();
        let second_url: Url = second_url.parse().unwrap();
        let first = key();
        let second = key();
        assert!(
            observe(temp.path(), &first_url, "registry.test", &first)
                .await
                .is_empty()
        );
        let warnings = observe(temp.path(), &second_url, "registry.test", &second).await;
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains(&fingerprint(&first.key)));
        assert!(warnings[0].contains(&fingerprint(&second.key)));
        assert!(
            observe(temp.path(), &first_url, "registry.test", &second)
                .await
                .is_empty()
        );
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }
}

#[tokio::test]
async fn temporary_identities_neither_create_nor_replace_history() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("identities");
    let url: Url = "https://registry.test/".parse().unwrap();
    let mut temporary = key();
    temporary.temporary = true;
    assert!(
        observe(&directory, &url, "registry.test", &temporary)
            .await
            .is_empty()
    );
    assert!(!directory.exists());
    let persistent = key();
    assert!(
        observe(&directory, &url, "registry.test", &persistent)
            .await
            .is_empty()
    );
    assert!(
        observe(&directory, &url, "registry.test", &temporary)
            .await
            .is_empty()
    );
    assert!(
        observe(&directory, &url, "registry.test", &persistent)
            .await
            .is_empty()
    );
    let warnings = observe(&directory, &url, "registry.test", &key()).await;
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains(&fingerprint(&persistent.key)));
}

#[tokio::test]
async fn identity_scope_keeps_selected_names_ports_and_prefixes_without_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let selected: Url = "https://registry.local:4433/catalog".parse().unwrap();
    let dial: Url = "https://user:secret@127.0.0.1:4433/catalog/?token=private"
        .parse()
        .unwrap();
    let first = key();
    let second = key();
    assert!(
        observe(temp.path(), &selected, "registry.local", &first)
            .await
            .is_empty()
    );
    let warning = observe(temp.path(), &dial, "registry.local", &second).await;
    assert_eq!(warning.len(), 1);
    assert!(warning[0].contains(selected.as_str()));
    for private in ["127.0.0.1", "secret", "private", "token="] {
        assert!(!warning[0].contains(private));
    }
    for source in [
        "https://registry.local:4434/catalog/",
        "https://registry.local:4433/another/",
    ] {
        let url = source.parse().unwrap();
        assert!(
            observe(temp.path(), &url, "registry.local", &first)
                .await
                .is_empty()
        );
    }
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 3);
}

#[test]
fn identity_targets_keep_ipv6_authorities_and_ports() {
    let url: Url = "https://[::1]:4433/prefix".parse().unwrap();
    assert_eq!(target(&url, "[::1]").unwrap(), url);
}

#[tokio::test]
async fn corrupt_history_warns_and_recovers_with_the_verified_identity() {
    let temp = tempfile::tempdir().unwrap();
    let url: Url = "http://registry.test/".parse().unwrap();
    let path = identity_file(temp.path(), &url);
    std::fs::write(&path, "not a key").unwrap();
    let peer = key();
    let warnings = observe(temp.path(), &url, "registry.test", &peer).await;
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("could not read"));
    assert!(
        observe(temp.path(), &url, "registry.test", &peer)
            .await
            .is_empty()
    );
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn storage_failures_are_advisories_not_call_errors() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("file");
    std::fs::write(&directory, "not a directory").unwrap();
    let url = "https://registry.test/".parse().unwrap();
    let warnings = observe(&directory, &url, "registry.test", &key()).await;
    assert!(
        warnings
            .iter()
            .any(|message| message.contains("could not remember"))
    );
    assert!(
        warnings
            .iter()
            .all(|message| message.contains("connection continues"))
    );
}
