use std::os::unix::fs::PermissionsExt;

use super::split_nmcli_terse_line;

#[test]
fn terse_line_unescapes_colon_and_backslash() {
    let line = r"my\:wifi\\x:61f7f9e2:802-11-wireless:";
    assert_eq!(
        split_nmcli_terse_line(line),
        vec![
            "my:wifi\\x".to_owned(),
            "61f7f9e2".to_owned(),
            "802-11-wireless".to_owned(),
            String::new(),
        ],
    );
}

#[test]
fn terse_line_plain_fields() {
    assert_eq!(
        split_nmcli_terse_line("café:90:WPA2 WPA3"),
        vec!["café".to_owned(), "90".to_owned(), "WPA2 WPA3".to_owned()],
    );
}

#[tokio::test]
async fn normalize_profiles_child() {
    if std::env::var_os("STARTOS_NM_PROFILE_FIXTURE").is_some() {
        super::force_ipv6_addr_gen_eui64().await;
    }
}

fn normalize_profiles(failure: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    for (profile, saved) in [
        ("wired", "eee-enabled=off\nmtu=1400\n"),
        ("wifi", "ssid=Saved network\n"),
        ("vpn", "private-key=fixture\n"),
    ] {
        std::fs::write(directory.path().join(format!("saved-{profile}")), saved).unwrap();
        std::fs::write(
            directory.path().join(format!("cached-{profile}")),
            "startup defaults\n",
        )
        .unwrap();
    }
    let script = directory.path().join("nmcli");
    std::fs::write(
        &script,
        r#"#!/bin/sh
set -eu
cd "$STARTOS_NM_PROFILE_FIXTURE"
printf '%s\n' "$*" >> commands
case "$*" in
    'connection reload')
        test "$STARTOS_NM_FAILURE" != reload || exit 1
        for profile in wired wifi vpn; do
            cp "saved-$profile" "cached-$profile"
        done
        ;;
    '-t -f UUID,TYPE connection show')
        test "$STARTOS_NM_FAILURE" != show || exit 1
        printf 'wired:802-3-ethernet\nwifi:802-11-wireless\nvpn:wireguard\n'
        ;;
    'connection modify '* )
        test "$4 $5 $6 $7" = 'ipv6.addr-gen-mode eui64 ipv6.ip6-privacy 0'
        cp "cached-$3" "saved-$3"
        printf 'ipv6.addr-gen-mode=eui64\nipv6.ip6-privacy=0\n' >> "saved-$3"
        ;;
    *) exit 2 ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(
        std::iter::once(directory.path().to_owned())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "net::wifi::tests::normalize_profiles_child",
            "--nocapture",
        ])
        .env("PATH", path)
        .env("STARTOS_NM_PROFILE_FIXTURE", directory.path())
        .env("STARTOS_NM_FAILURE", failure)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    directory
}

#[test]
fn ipv6_normalization_preserves_saved_physical_profiles() {
    let directory = normalize_profiles("");
    for (profile, expected) in [
        (
            "wired",
            "eee-enabled=off\nmtu=1400\nipv6.addr-gen-mode=eui64\nipv6.ip6-privacy=0\n",
        ),
        (
            "wifi",
            "ssid=Saved network\nipv6.addr-gen-mode=eui64\nipv6.ip6-privacy=0\n",
        ),
        ("vpn", "private-key=fixture\n"),
    ] {
        assert_eq!(
            std::fs::read_to_string(directory.path().join(format!("saved-{profile}"))).unwrap(),
            expected
        );
    }
}

#[test]
fn failed_profile_reload_leaves_saved_settings_untouched() {
    let directory = normalize_profiles("reload");
    assert_eq!(
        std::fs::read_to_string(directory.path().join("commands")).unwrap(),
        "connection reload\n"
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("saved-wired")).unwrap(),
        "eee-enabled=off\nmtu=1400\n"
    );
}

#[test]
fn failed_profile_listing_does_not_modify_saved_settings() {
    let directory = normalize_profiles("show");
    assert_eq!(
        std::fs::read_to_string(directory.path().join("commands")).unwrap(),
        "connection reload\n-t -f UUID,TYPE connection show\n"
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("saved-wired")).unwrap(),
        "eee-enabled=off\nmtu=1400\n"
    );
}
