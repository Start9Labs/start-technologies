use super::*;
use crate::net::wifi::{NetworkProfile, NetworkProfiles};

async fn transported(profiles: &NetworkProfiles) -> NetworkProfiles {
    let file = anonymous_file().unwrap().into_std().await;
    let mut archive = tar::Builder::new(file);
    let mut header = tar::Header::new_gnu();
    header.set_size(0);
    header.set_mode(0o600);
    header.set_cksum();
    archive
        .append_data(&mut header, CAPTURE_FILE, std::io::empty())
        .unwrap();
    append_json(&mut archive, KEYS_FILE, &Vec::<OsString>::new()).unwrap();
    append_json(
        &mut archive,
        BACKINGS_FILE,
        &std::collections::BTreeSet::<Guid>::new(),
    )
    .unwrap();
    append_profiles(&mut archive, profiles).unwrap();
    receive(tokio::fs::File::from_std(archive.into_inner().unwrap()))
        .await
        .unwrap()
        .2
}

#[tokio::test]
async fn raw_profile_transport_preserves_escaped_large_contents_and_nanosecond_times() {
    let contents = "[wifi-security]\npsk=\"quoted\\private-κλειδί\"\n".repeat(8192);
    let profiles = NetworkProfiles::from([(
        "legacy".to_owned(),
        NetworkProfile {
            contents: Zeroizing::new(contents.clone()),
            modified: (-1, 987_654_321),
        },
    )]);
    let restored = transported(&profiles).await;
    assert_eq!(&*restored["legacy"].contents, &contents);
    assert_eq!(restored["legacy"].modified, (-1, 987_654_321));
    assert!(transported(&NetworkProfiles::new()).await.is_empty());
}

#[tokio::test]
async fn profile_materialization_preserves_duplicate_uuid_selection_inputs() {
    use std::os::unix::fs::MetadataExt;

    let root = tempfile::tempdir().unwrap();
    let saved = root.path().join("saved");
    let restored = root.path().join("restored");
    tokio::fs::create_dir_all(&saved).await.unwrap();
    for (name, nanoseconds) in [("a-winner", 900_000_001), ("z-loser", 100_000_003)] {
        NetworkProfile {
            contents: Zeroizing::new(
                "[connection]\nuuid=9c07caf8-4c53-43a4-aa3f-9a26c0acc529\n".to_owned(),
            ),
            modified: (1_700_000_000, nanoseconds),
        }
        .install_at(&saved.join(name))
        .await
        .unwrap();
    }
    let captured = crate::net::wifi::read_network_profiles(&saved)
        .await
        .unwrap();
    let mut credentials = super::super::super::credentials::test_credentials();
    credentials.network_profiles = transported(&captured).await;
    let key = super::super::super::bootstrap::generate_key().unwrap();
    let credentials = super::super::super::credentials::Credentials::decrypt(
        &credentials.encrypt(&key).unwrap(),
        &key,
    )
    .unwrap();
    credentials.install_profiles_at(&restored).await.unwrap();
    for name in captured.keys() {
        let original = std::fs::metadata(saved.join(name)).unwrap();
        let copy = std::fs::metadata(restored.join(name)).unwrap();
        assert_eq!(
            (copy.mtime(), copy.mtime_nsec()),
            (original.mtime(), original.mtime_nsec())
        );
        assert_eq!(copy.mode() & 0o777, 0o600);
        assert_eq!(
            std::fs::read(saved.join(name)).unwrap(),
            std::fs::read(restored.join(name)).unwrap()
        );
    }
}

#[test]
fn exact_secret_ranges_preserve_the_shared_cursor_and_reject_truncation() {
    use std::io::{Seek, Write};

    let mut file = tempfile::tempfile().unwrap();
    file.write_all(b"prefix-private-profile-suffix").unwrap();
    file.rewind().unwrap();
    let bytes = crate::util::io::read_secret_at(&file, 7, 15).unwrap();
    assert_eq!(&*bytes, b"private-profile");
    assert_eq!(file.stream_position().unwrap(), 0);
    assert_eq!(bytes.capacity(), bytes.len());
    assert!(crate::util::io::read_secret_at(&file, 29, 1).is_err());
    assert!(
        crate::util::io::read_secret_at(&file, 0, 0)
            .unwrap()
            .is_empty()
    );
}

async fn native_profile_id(directory: &Path, uuid: &str) -> String {
    use std::time::Duration;

    let state = tempfile::tempdir().unwrap();
    let socket = state.path().join("bus");
    let address = format!("unix:path={}", socket.display());
    let mut bus = Command::new("/usr/bin/dbus-daemon")
        .args(["--session", "--nofork", "--nopidfile"])
        .arg(format!("--address={address}"))
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let config = state.path().join("NetworkManager.conf");
    std::fs::write(
        &config,
        format!(
            "[main]\nplugins=keyfile\n[keyfile]\npath={}\nunmanaged-devices=*\n",
            directory.display(),
        ),
    )
    .unwrap();
    let mut manager = Command::new("/usr/sbin/NetworkManager")
        .arg("--no-daemon")
        .arg("--config")
        .arg(&config)
        .arg("--pid-file")
        .arg(state.path().join("pid"))
        .arg("--state-file")
        .arg(state.path().join("state"))
        .env("DBUS_SYSTEM_BUS_ADDRESS", &address)
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Ok(bytes) = Command::new("/usr/bin/nmcli")
                .args([
                    "--get-values",
                    "connection.id",
                    "connection",
                    "show",
                    "uuid",
                    uuid,
                ])
                .env("DBUS_SYSTEM_BUS_ADDRESS", &address)
                .timeout(Some(Duration::from_secs(2)))
                .invoke(ErrorKind::Network)
                .await
            {
                break String::from_utf8(bytes).unwrap().trim().to_owned();
            }
            assert!(
                manager.try_wait().unwrap().is_none(),
                "native NetworkManager exited"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    manager.kill().await.unwrap();
    manager.wait().await.unwrap();
    bus.kill().await.unwrap();
    bus.wait().await.unwrap();
    result.unwrap()
}

#[tokio::test]
#[ignore = "requires the isolated native NetworkManager fixture"]
async fn native_networkmanager_keeps_the_same_duplicate_uuid_winner_after_transport() {
    assert_eq!(std::env::var("FDE_NM_TEST").unwrap(), "1");
    let root = tempfile::tempdir().unwrap();
    let saved = root.path().join("saved");
    let restored = root.path().join("restored");
    std::fs::create_dir(&saved).unwrap();
    let uuid = "9c07caf8-4c53-43a4-aa3f-9a26c0acc529";
    for (name, id, nanoseconds) in [
        ("a-winner", "mtime-winner", 900_000_001),
        ("z-loser", "wrong-winner", 100_000_003),
    ] {
        NetworkProfile {
            contents: Zeroizing::new(format!(
                "[connection]\nid={id}\nuuid={uuid}\ntype=ethernet\nautoconnect=false\n[ethernet]\n"
            )),
            modified: (1_700_000_000, nanoseconds),
        }
        .install_at(&saved.join(name))
        .await
        .unwrap();
    }
    let expected = native_profile_id(&saved, uuid).await;
    assert_eq!(expected, "mtime-winner");
    let captured = crate::net::wifi::read_network_profiles(&saved)
        .await
        .unwrap();
    let lost = root.path().join("lost-timestamps");
    std::fs::create_dir(&lost).unwrap();
    for (name, profile) in &captured {
        crate::util::io::write_file_private_atomic(lost.join(name), &profile.contents)
            .await
            .unwrap();
    }
    assert_eq!(native_profile_id(&lost, uuid).await, "wrong-winner");
    let mut credentials = super::super::super::credentials::test_credentials();
    credentials.network_profiles = transported(&captured).await;
    credentials.install_profiles_at(&restored).await.unwrap();
    assert_eq!(native_profile_id(&restored, uuid).await, expected);
}
