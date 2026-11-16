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

const WIRED_UUID: &str = "11111111-2222-3333-4444-555555555555";
const OTHER_UUID: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

fn keyfile(uuid: &str, kind: &str, metadata: &str) -> String {
    format!("[connection]\nid=fixture\nuuid={uuid}\ntype={kind}\n\n[.nmmeta]\n{metadata}")
}

#[test]
fn wired_profile_metadata_distinguishes_auto_defaults() {
    for (kind, metadata, expected) in [
        ("ethernet", "nm-generated=true\n", Some(true)),
        ("ethernet", "", Some(false)),
        ("ethernet", "nm-generated=false\n", Some(false)),
        (
            "ethernet",
            "nm-generated=true\nexternal=true\n",
            Some(false),
        ),
        (
            "ethernet",
            "nm-generated=true\nvolatile=true\n",
            Some(false),
        ),
        ("bridge", "nm-generated=true\n", None),
        ("loopback", "nm-generated=true\n", None),
        ("wifi", "nm-generated=true\n", None),
    ] {
        let contents = keyfile(WIRED_UUID, kind, metadata);
        let actual = super::parse_wired_profile(&contents)
            .map(|profile| (profile.uuid.to_string(), profile.auto_default));
        assert_eq!(
            actual,
            expected.map(|generated| (WIRED_UUID.to_owned(), generated)),
            "{contents}"
        );
    }
    assert!(super::parse_wired_profile(&keyfile("invalid", "ethernet", "")).is_none());
    let contents = format!("[connection]\nuuid={WIRED_UUID}\ntype=ethernet\nnm-generated=true\n");
    assert!(!super::parse_wired_profile(&contents).unwrap().auto_default);
}

#[tokio::test]
async fn runtime_cleanup_removes_only_defaults_shadowing_saved_wired_profiles() {
    let directory = tempfile::tempdir().unwrap();
    let runtime = directory.path().join("runtime");
    let saved = directory.path().join("saved");
    std::fs::create_dir(&runtime).unwrap();
    std::fs::create_dir(&saved).unwrap();
    std::fs::write(
        saved.join("wired.nmconnection"),
        keyfile(WIRED_UUID, "ethernet", ""),
    )
    .unwrap();
    std::fs::write(
        saved.join("wifi.nmconnection"),
        keyfile(OTHER_UUID, "wifi", ""),
    )
    .unwrap();
    std::fs::write(
        saved.join("backup.nmconnection.bak"),
        keyfile(OTHER_UUID, "ethernet", ""),
    )
    .unwrap();
    for (name, uuid, kind, metadata) in [
        ("shadow", WIRED_UUID, "ethernet", "nm-generated=true\n"),
        ("unmatched", OTHER_UUID, "ethernet", "nm-generated=true\n"),
        ("user", WIRED_UUID, "ethernet", ""),
        (
            "external",
            WIRED_UUID,
            "ethernet",
            "nm-generated=true\nexternal=true\n",
        ),
        (
            "volatile",
            WIRED_UUID,
            "ethernet",
            "nm-generated=true\nvolatile=true\n",
        ),
        ("bridge", WIRED_UUID, "bridge", "nm-generated=true\n"),
        ("loopback", WIRED_UUID, "loopback", "nm-generated=true\n"),
    ] {
        std::fs::write(
            runtime.join(format!("{name}.nmconnection")),
            keyfile(uuid, kind, metadata),
        )
        .unwrap();
    }
    super::remove_shadowing_wired_profiles(&runtime, &saved)
        .await
        .unwrap();
    for (name, exists) in [
        ("shadow", false),
        ("unmatched", true),
        ("user", true),
        ("external", true),
        ("volatile", true),
        ("bridge", true),
        ("loopback", true),
    ] {
        assert_eq!(
            runtime.join(format!("{name}.nmconnection")).exists(),
            exists,
            "{name}"
        );
    }
}

#[tokio::test]
async fn no_saved_profiles_leave_generated_wired_profile_untouched() {
    let directory = tempfile::tempdir().unwrap();
    let saved = directory.path().join("saved");
    std::fs::create_dir(&saved).unwrap();
    let generated = directory.path().join("default.nmconnection");
    std::fs::write(
        &generated,
        keyfile(WIRED_UUID, "ethernet", "nm-generated=true\n"),
    )
    .unwrap();
    super::remove_shadowing_wired_profiles(directory.path(), &saved)
        .await
        .unwrap();
    assert!(generated.exists());
}

#[tokio::test]
async fn missing_runtime_profile_directory_needs_no_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    super::remove_shadowing_wired_profiles(&directory.path().join("missing"), directory.path())
        .await
        .unwrap();
}

#[tokio::test]
async fn normalize_profiles_child() {
    if let Some(directory) = std::env::var_os("STARTOS_NM_PROFILE_FIXTURE") {
        let directory = std::path::Path::new(&directory);
        let product = std::env::var("STARTOS_NM_PRODUCT").ok();
        super::prepare_physical_profiles(
            &directory.join("runtime"),
            &directory.join("saved-profiles"),
            product.as_deref(),
        )
        .await;
    }
}

fn normalize_profiles(failure: &str, product: Option<&str>, eee: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let runtime = directory.path().join("runtime");
    let saved = directory.path().join("saved-profiles");
    std::fs::create_dir(&runtime).unwrap();
    std::fs::create_dir(&saved).unwrap();
    std::fs::write(
        saved.join("wired.nmconnection"),
        keyfile(WIRED_UUID, "ethernet", ""),
    )
    .unwrap();
    std::fs::write(
        runtime.join("generated.nmconnection"),
        keyfile(WIRED_UUID, "ethernet", "nm-generated=true\n"),
    )
    .unwrap();
    if failure == "cleanup" {
        std::fs::write(runtime.join("unreadable.nmconnection"), [0xff]).unwrap();
    }
    let wired = if eee.is_empty() {
        "mtu=1400\n".to_owned()
    } else {
        format!("eee-enabled={eee}\nmtu=1400\n")
    };
    std::fs::write(directory.path().join("eee-setting"), eee).unwrap();
    for (profile, saved) in [
        ("wired", wired.as_str()),
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
            if test "$profile" = wired && test -f runtime/generated.nmconnection; then
                continue
            fi
            cp "saved-$profile" "cached-$profile"
        done
        ;;
    '-t -f UUID,TYPE connection show')
        test "$STARTOS_NM_FAILURE" != show || exit 1
        printf 'wired:802-3-ethernet\nwifi:802-11-wireless\nvpn:wireguard\n'
        ;;
    '-g ethtool.eee-enabled connection show uuid wired')
        test "$STARTOS_NM_FAILURE" != eee || exit 1
        cat eee-setting
        ;;
    'connection modify '* )
        test "$4 $5 $6 $7" = 'ipv6.addr-gen-mode eui64 ipv6.ip6-privacy 0'
        cp "cached-$3" "saved-$3"
        printf 'ipv6.addr-gen-mode=eui64\nipv6.ip6-privacy=0\n' >> "saved-$3"
        if test "$#" -gt 7; then
            test "$3 $8 $9" = 'wired ethtool.eee-enabled off'
            printf 'eee-enabled=off\n' >> "saved-$3"
        fi
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
        .env("STARTOS_NM_PRODUCT", product.unwrap_or_default())
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
    let directory = normalize_profiles("", None, "off");
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
fn failed_runtime_cleanup_prevents_profile_modification() {
    let directory = normalize_profiles("cleanup", None, "off");
    assert!(!directory.path().join("commands").exists());
    assert_eq!(
        std::fs::read_to_string(directory.path().join("saved-wired")).unwrap(),
        "eee-enabled=off\nmtu=1400\n"
    );
}

#[test]
fn failed_profile_reload_leaves_saved_settings_untouched() {
    let directory = normalize_profiles("reload", None, "off");
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
fn server_pure_defaults_ethernet_eee_off_without_changing_wifi() {
    let directory = normalize_profiles("", Some("librem_mini_v2"), "");
    let wired = std::fs::read_to_string(directory.path().join("saved-wired")).unwrap();
    assert_eq!(
        wired,
        "mtu=1400\nipv6.addr-gen-mode=eui64\nipv6.ip6-privacy=0\neee-enabled=off\n"
    );
    let wifi = std::fs::read_to_string(directory.path().join("saved-wifi")).unwrap();
    assert_eq!(
        wifi,
        "ssid=Saved network\nipv6.addr-gen-mode=eui64\nipv6.ip6-privacy=0\n"
    );
    let commands = std::fs::read_to_string(directory.path().join("commands")).unwrap();
    assert_eq!(
        commands
            .lines()
            .filter(|line| line.starts_with("-g ethtool.eee-enabled"))
            .count(),
        1
    );
}

#[test]
fn server_pure_preserves_explicit_eee_selections() {
    for eee in ["on", "off"] {
        let directory = normalize_profiles("", Some("librem_mini_v2"), eee);
        let wired = std::fs::read_to_string(directory.path().join("saved-wired")).unwrap();
        assert!(wired.starts_with(&format!("eee-enabled={eee}\n")));
        assert_eq!(wired.matches("eee-enabled=").count(), 1);
        let commands = std::fs::read_to_string(directory.path().join("commands")).unwrap();
        assert!(!commands.contains(" ethtool.eee-enabled off"));
    }
}

#[test]
fn other_or_unknown_hardware_keeps_eee_unmanaged() {
    for product in [Some("other"), None] {
        let directory = normalize_profiles("", product, "");
        let wired = std::fs::read_to_string(directory.path().join("saved-wired")).unwrap();
        assert!(!wired.contains("eee-enabled"));
        let commands = std::fs::read_to_string(directory.path().join("commands")).unwrap();
        assert!(!commands.contains("ethtool.eee-enabled"));
    }
}

#[test]
fn failed_eee_query_is_not_treated_as_an_unset_preference() {
    let directory = normalize_profiles("eee", Some("librem_mini_v2"), "");
    let wired = std::fs::read_to_string(directory.path().join("saved-wired")).unwrap();
    assert_eq!(
        wired,
        "mtu=1400\nipv6.addr-gen-mode=eui64\nipv6.ip6-privacy=0\n"
    );
    let commands = std::fs::read_to_string(directory.path().join("commands")).unwrap();
    assert!(!commands.contains(" ethtool.eee-enabled off"));
}

#[test]
fn failed_profile_listing_does_not_modify_saved_settings() {
    let directory = normalize_profiles("show", None, "off");
    assert_eq!(
        std::fs::read_to_string(directory.path().join("commands")).unwrap(),
        "connection reload\n-t -f UUID,TYPE connection show\n"
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("saved-wired")).unwrap(),
        "eee-enabled=off\nmtu=1400\n"
    );
}
