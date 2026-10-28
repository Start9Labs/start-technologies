//! One-time upgrade migrations, run at boot before `network` starts.
//!
//! `/etc/startwrt/migrations` records how many entries of [`MIGRATIONS`] this
//! router's configuration has been through. sysupgrade, the setup wizard's
//! Update flash and config backups carry it with the rest of the configuration.

use std::path::{Path, PathBuf};

use tokio::io::AsyncWriteExt;
use uciedit::openwrt::{Dhcp, InterfaceProto, NetworkInterface};
use uciedit::{dump_all, parse_all, Arena, TypedSection};

use crate::prelude::*;
use crate::wan::WAN_INTERFACE;
use crate::ServerContext;

const RECORD: &str = "etc/startwrt/migrations";

/// Ordered oldest first. Append only: a record counts entries of this list.
///
/// Each migration must leave correct a configuration it already migrated, or
/// one from any earlier release. The runner replays migrations after power
/// loss before the record advances, after a restored backup without a
/// record, and on a pre-runner install.
const MIGRATIONS: &[Migration] = &[
    Migration::RemoveEtcNftablesRules,
    Migration::StripUnreferencedWanZoneVpns,
    Migration::PppoeWanIpv6,
    Migration::StoreLanAccessMode,
    Migration::ExpandLogRetention,
    Migration::StoreLogsUnderVar,
    Migration::RemoveSessionsJson,
    Migration::ForceDhcpPools,
    Migration::RemoveServerCert,
];

#[derive(Clone, Copy, Debug)]
enum Migration {
    /// Earlier images staged their nft includes in `/etc/nftables.d`, which
    /// fw4 loads alongside the ones in `/usr/share/nftables.d`.
    RemoveEtcNftablesRules,
    /// VPN outbounds once lived directly in the wan zone.
    StripUnreferencedWanZoneVpns,
    /// Images without PPP saved PPPoE WANs with `ipv6` unset, which pppd reads
    /// as `auto` and answers with a second DHCPv6 client beside `wan6`.
    PppoeWanIpv6,
    /// Earlier releases kept a profile's LAN Access only as its forwardings.
    StoreLanAccessMode,
    /// Earlier images capped the persistent log at 512 KiB.
    ExpandLogRetention,
    /// Earlier images kept `/var` in RAM, the system log in `/etc/log` and
    /// the activity log in `/etc/startwrt`.
    StoreLogsUnderVar,
    RemoveSessionsJson,
    /// Earlier releases probed for another DHCP server before serving a pool.
    ForceDhcpPools,
    /// Earlier images served one certificate from `/etc/ssl`.
    RemoveServerCert,
}

impl Migration {
    async fn apply(self, root: &Path) -> Result<(), Error> {
        match self {
            Migration::RemoveEtcNftablesRules => remove_etc_nftables_rules(root).await,
            Migration::StripUnreferencedWanZoneVpns => strip_unreferenced_wan_zone_vpns(root).await,
            Migration::PppoeWanIpv6 => pppoe_wan_ipv6(root).await,
            Migration::StoreLanAccessMode => store_lan_access_mode(root).await,
            Migration::ExpandLogRetention => expand_log_retention(root).await,
            Migration::StoreLogsUnderVar => store_logs_under_var(root).await,
            Migration::RemoveSessionsJson => remove_sessions_json(root).await,
            Migration::ForceDhcpPools => force_dhcp_pools(root).await,
            Migration::RemoveServerCert => remove_server_cert(root).await,
        }
    }
}

/// Applies every migration the configuration under the filesystem root has
/// not been through, advancing the record after each one.
///
/// Without a record, a configuration that StartWRT has already run on starts
/// from the first migration and a fresh one is recorded as current. A failed
/// migration stops the run and stays pending for the next boot.
pub async fn run(root: &Path) -> Result<(), Error> {
    let record = root.join(RECORD);
    let mut applied = match read_record(&record).await? {
        Some(applied) => applied,
        None if has_run_startwrt(root).await => 0,
        None => {
            write_record(&record, MIGRATIONS.len()).await?;
            return Ok(());
        }
    };
    while let Some(migration) = MIGRATIONS.get(applied) {
        migration.apply(root).await.map_err(|e| {
            Error::new(
                eyre!("migration {migration:?} failed: {e}"),
                ErrorKind::Filesystem,
            )
        })?;
        applied += 1;
        write_record(&record, applied).await?;
    }
    Ok(())
}

/// Drops the record when a restored backup did not include one, so the next
/// boot migrates the restored configuration.
///
/// Takes the backup's `tar -t` listing.
pub async fn forget_unless_restored(root: &Path, archive_listing: &str) -> Result<(), Error> {
    if archive_listing
        .lines()
        .any(|entry| entry.trim_start_matches("./").trim_start_matches('/') == RECORD)
    {
        return Ok(());
    }
    match tokio::fs::remove_file(root.join(RECORD)).await {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// The root CA marks a configuration the daemon has started on.
///
/// Holds while only the daemon creates the CA, after this runner at boot.
/// A CA created earlier makes every fresh router replay every migration.
async fn has_run_startwrt(root: &Path) -> bool {
    let ca = crate::ssl::ca_cert_path();
    tokio::fs::try_exists(root.join(ca.strip_prefix("/").unwrap_or(&ca)))
        .await
        .unwrap_or(false)
}

async fn read_record(record: &Path) -> Result<Option<usize>, Error> {
    match tokio::fs::read_to_string(record).await {
        Ok(text) => text.trim().parse().map(Some).map_err(|e| {
            Error::new(
                eyre!("{}: invalid migration record: {e}", record.display()),
                ErrorKind::Filesystem,
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

async fn write_record(record: &Path, applied: usize) -> Result<(), Error> {
    startos::util::io::write_file_atomic(record, format!("{applied}\n"))
        .await
        .map_err(Error::from)
}

fn uci_root(root: &Path) -> PathBuf {
    root.join("etc/config")
}

async fn remove_etc_nftables_rules(root: &Path) -> Result<(), Error> {
    for rule in [
        "10-startwrt-dnat-mark.nft",
        "11-startwrt-inbound6-mark.nft",
        "12-startwrt-sni-divert.nft",
        "13-startwrt-dns-update-divert.nft",
    ] {
        match tokio::fs::remove_file(root.join("etc/nftables.d").join(rule)).await {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    }
    Ok(())
}

async fn remove_sessions_json(root: &Path) -> Result<(), Error> {
    match tokio::fs::remove_file(root.join("etc/startwrt/sessions.json")).await {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

async fn force_dhcp_pools(root: &Path) -> Result<(), Error> {
    let arena = Arena::new();
    let mut cfgs = parse_all(uci_root(root), &arena, &["dhcp"]).await?;
    let mut changed = false;
    for section in &mut cfgs["dhcp"].sections {
        let Ok(mut dhcp) = section.get::<Dhcp>() else {
            continue;
        };
        if dhcp.force != Some(true) {
            dhcp.force = Some(true);
            section.set(&dhcp)?;
            changed = true;
        }
    }
    if changed {
        dump_all(uci_root(root), cfgs).await?;
    }
    Ok(())
}

async fn remove_server_cert(root: &Path) -> Result<(), Error> {
    for path in [
        "etc/ssl/certs/startwrt-server.pem",
        "etc/ssl/private/startwrt-server.key",
    ] {
        match tokio::fs::remove_file(root.join(path)).await {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    }
    Ok(())
}

async fn strip_unreferenced_wan_zone_vpns(root: &Path) -> Result<(), Error> {
    let arena = Arena::new();
    let mut cfgs = parse_all(uci_root(root), &arena, &["firewall", "startwrt"]).await?;
    if crate::profiles::strip_unreferenced_wan_zone_vpns(&mut cfgs)? {
        dump_all(uci_root(root), cfgs).await?;
    }
    Ok(())
}

/// An explicit `ipv6` is a later choice and stays.
async fn pppoe_wan_ipv6(root: &Path) -> Result<(), Error> {
    let arena = Arena::new();
    let mut cfgs = parse_all(uci_root(root), &arena, &["network"]).await?;
    let mut changed = false;
    for section in &mut cfgs["network"].sections {
        if section.name().as_deref() != Some(WAN_INTERFACE) {
            continue;
        }
        if let Some(mut wan) = section.get_typed::<NetworkInterface>()? {
            if wan.proto == InterfaceProto::PPPOE && wan.ipv6.is_none() {
                wan.ipv6 = Some("1".to_string());
                section.set(&wan)?;
                changed = true;
            }
        }
        break;
    }
    if changed {
        dump_all(uci_root(root), cfgs).await?;
    }
    Ok(())
}

async fn store_lan_access_mode(root: &Path) -> Result<(), Error> {
    let arena = Arena::new();
    let mut cfgs = parse_all(uci_root(root), &arena, &["startwrt", "firewall"]).await?;
    if crate::profiles::migrate_lan_access_mode(ServerContext::default(), &mut cfgs)? {
        dump_all(uci_root(root), cfgs).await?;
    }
    Ok(())
}

#[derive(Debug, TypedSection, Default)]
#[uci(ty = "system")]
struct SystemLog {
    log_file: Option<String>,
    log_size: Option<String>,
    log_buffer_size: Option<String>,
}

/// A size other than the shipped one is a later choice and stays.
async fn expand_log_retention(root: &Path) -> Result<(), Error> {
    let arena = Arena::new();
    let mut cfgs = parse_all(uci_root(root), &arena, &["system"]).await?;
    let mut changed = false;
    for section in &mut cfgs["system"].sections {
        if let Some(mut log) = section.get_typed::<SystemLog>()? {
            if log.log_size.as_deref() == Some("512") && log.log_buffer_size.is_none() {
                log.log_size = Some("32768".to_string());
                log.log_buffer_size = Some("512".to_string());
                section.set(&log)?;
                changed = true;
            }
            break;
        }
    }
    if changed {
        dump_all(uci_root(root), cfgs).await?;
    }
    Ok(())
}

const LEGACY_ACTIVITY_DB: &str = "etc/startwrt/activity.db";

/// Lines the follower wrote to the old file on this boot follow a log
/// already at the new location. A log file other than the shipped one stays.
async fn store_logs_under_var(root: &Path) -> Result<(), Error> {
    for (from, to) in crate::logs::LEGACY_HISTORY_FILES
        .into_iter()
        .zip(crate::logs::HISTORY_FILES)
    {
        move_file(&root.join(from), &root.join(to), Collision::Append).await?;
    }
    let _ = tokio::fs::remove_dir(root.join("etc/log")).await;
    move_file(
        &root.join(LEGACY_ACTIVITY_DB),
        &root.join(crate::activity::ACTIVITY_DB.trim_start_matches('/')),
        Collision::Keep,
    )
    .await?;

    let arena = Arena::new();
    let mut cfgs = parse_all(uci_root(root), &arena, &["system"]).await?;
    let mut changed = false;
    for section in &mut cfgs["system"].sections {
        if let Some(mut log) = section.get_typed::<SystemLog>()? {
            if log.log_file.as_deref() == Some("/etc/log/messages") {
                log.log_file = Some("/var/log/messages".to_string());
                section.set(&log)?;
                changed = true;
            }
            break;
        }
    }
    if changed {
        dump_all(uci_root(root), cfgs).await?;
    }
    Ok(())
}

/// What becomes of a file moved onto one already at the destination.
#[derive(Clone, Copy)]
enum Collision {
    /// Its contents follow the destination's.
    Append,
    /// It stays where it was.
    Keep,
}

/// A missing source is a no-op. Crosses filesystems.
async fn move_file(from: &Path, to: &Path, collision: Collision) -> Result<(), Error> {
    match tokio::fs::symlink_metadata(from).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    }
    if let Some(parent) = to.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if tokio::fs::try_exists(to).await? {
        match collision {
            Collision::Keep => return Ok(()),
            Collision::Append => {
                let moved = tokio::fs::read(from).await?;
                let mut dest = tokio::fs::OpenOptions::new().append(true).open(to).await?;
                dest.write_all(&moved).await?;
                dest.flush().await?;
                tokio::fs::remove_file(from).await?;
                return Ok(());
            }
        }
    }
    if tokio::fs::rename(from, to).await.is_ok() {
        return Ok(());
    }
    tokio::fs::copy(from, to).await?;
    tokio::fs::remove_file(from).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIPPED_SYSTEM: &str = "\
config system
\toption hostname 'router'
\toption log_size '512'
\toption log_file '/etc/log/messages'
";

    const PPPOE_WAN: &str = "\
config interface 'wan'
\toption device 'eth1'
\toption proto 'pppoe'
\toption username 'user'
\toption password 'pass'
";

    const LEGACY_DHCP: &str = "\
config dnsmasq
\toption domain 'lan'

config dhcp 'lan'
\toption interface 'lan'
\toption start '2'
\toption limit '198'
\toption leasetime '12h'

config dhcp 'wan'
\toption interface 'wan'
\toption ignore '1'

config dhcp 'guest'
\toption interface 'guest'
\toption start '2'
\toption limit '198'
\toption leasetime '12h'
\toption force '0'
";

    fn router(record: Option<&str>, has_run: bool) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("etc/config")).unwrap();
        std::fs::create_dir_all(dir.path().join("etc/startwrt")).unwrap();
        if let Some(record) = record {
            std::fs::write(dir.path().join(RECORD), record).unwrap();
        }
        if has_run {
            let ca = crate::ssl::ca_cert_path();
            let ca = dir.path().join(ca.strip_prefix("/").unwrap());
            std::fs::create_dir_all(ca.parent().unwrap()).unwrap();
            std::fs::write(ca, "ca").unwrap();
        }
        dir
    }

    fn write(dir: &tempfile::TempDir, path: &str, text: &str) {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn read(dir: &tempfile::TempDir, path: &str) -> String {
        std::fs::read_to_string(dir.path().join(path)).unwrap()
    }

    fn record(dir: &tempfile::TempDir) -> String {
        read(dir, RECORD)
    }

    fn wan_ipv6(dir: &tempfile::TempDir) -> Option<String> {
        read(dir, "etc/config/network")
            .lines()
            .find_map(|l| l.trim().strip_prefix("option ipv6 "))
            .map(|v| v.trim_matches('\'').to_string())
    }

    const LEGACY_PROFILES: &str = "\
config profile 'lan'
\toption fullname 'Admin'
\toption interface 'lan'
\toption vlan_tag '99'
\toption access_to_new_profiles '1'

config profile 'guest'
\toption fullname 'Guest'
\toption interface 'guest'
\toption vlan_tag '101'
";

    const LEGACY_PROFILE_ZONES: &str = "
config zone
\toption name 'lan'
\toption input 'ACCEPT'
\toption output 'ACCEPT'
\toption forward 'ACCEPT'
\tlist network 'lan'

config zone
\toption name 'vlan_guest'
\toption input 'ACCEPT'
\toption output 'ACCEPT'
\toption forward 'ACCEPT'
\tlist network 'guest'

config forwarding
\toption src 'lan'
\toption dest 'vlan_guest'
";

    fn current() -> String {
        format!("{}\n", MIGRATIONS.len())
    }

    #[tokio::test]
    async fn log_migration_record_runs_session_cleanup_next() {
        let dir = router(Some("6\n"), true);
        write(&dir, "etc/startwrt/sessions.json", "{}");
        write(&dir, "etc/startwrt/auth.db", "enrolled keys");
        run(dir.path()).await.unwrap();
        assert!(!dir.path().join("etc/startwrt/sessions.json").exists());
        assert_eq!(read(&dir, "etc/startwrt/auth.db"), "enrolled keys");
        assert_eq!(read(&dir, RECORD), current());
    }

    #[tokio::test]
    async fn fresh_config_is_recorded_current_without_migrating() {
        let dir = router(None, false);
        write(&dir, "etc/config/network", PPPOE_WAN);
        run(dir.path()).await.unwrap();
        assert_eq!(record(&dir), current());
        assert_eq!(wan_ipv6(&dir), None);
    }

    #[tokio::test]
    async fn pre_runner_install_runs_every_migration() {
        let dir = router(None, true);
        write(&dir, "etc/config/network", PPPOE_WAN);
        write(&dir, "etc/nftables.d/12-startwrt-sni-divert.nft", "");
        write(&dir, "etc/nftables.d/user.nft", "");
        write(
            &dir,
            "etc/config/firewall",
            "config zone 'wan'\n\toption name 'wan'\n\toption input 'REJECT'\n\toption output 'ACCEPT'\n\toption forward 'REJECT'\n\tlist network 'wan'\n\tlist network 'wg_old'\n",
        );
        run(dir.path()).await.unwrap();
        assert_eq!(record(&dir), current());
        assert_eq!(wan_ipv6(&dir).as_deref(), Some("1"));
        assert!(!dir
            .path()
            .join("etc/nftables.d/12-startwrt-sni-divert.nft")
            .exists());
        assert!(dir.path().join("etc/nftables.d/user.nft").exists());
        let firewall = read(&dir, "etc/config/firewall");
        assert!(!firewall.contains("wg_old"));
        assert!(firewall.contains("list network 'wan'"));
    }

    #[tokio::test]
    async fn every_migration_is_safe_to_rerun() {
        let dir = router(None, true);
        write(&dir, "etc/config/network", PPPOE_WAN);
        write(&dir, "etc/nftables.d/12-startwrt-sni-divert.nft", "");
        write(
            &dir,
            "etc/config/firewall",
            &format!("config zone 'wan'\n\toption name 'wan'\n\toption input 'REJECT'\n\toption output 'ACCEPT'\n\toption forward 'REJECT'\n\tlist network 'wan'\n\tlist network 'wg_old'\n{LEGACY_PROFILE_ZONES}"),
        );
        write(&dir, "etc/config/startwrt", LEGACY_PROFILES);
        write(&dir, "etc/config/system", SHIPPED_SYSTEM);
        write(&dir, "etc/config/dhcp", LEGACY_DHCP);
        for migration in MIGRATIONS {
            migration.apply(dir.path()).await.unwrap();
        }
        let migrated = [
            read(&dir, "etc/config/network"),
            read(&dir, "etc/config/firewall"),
            read(&dir, "etc/config/startwrt"),
            read(&dir, "etc/config/system"),
            read(&dir, "etc/config/dhcp"),
        ];
        for migration in MIGRATIONS {
            migration.apply(dir.path()).await.unwrap();
        }
        assert_eq!(read(&dir, "etc/config/network"), migrated[0]);
        assert_eq!(read(&dir, "etc/config/firewall"), migrated[1]);
        assert_eq!(read(&dir, "etc/config/startwrt"), migrated[2]);
        assert_eq!(read(&dir, "etc/config/system"), migrated[3]);
        assert_eq!(read(&dir, "etc/config/dhcp"), migrated[4]);
        assert_eq!(wan_ipv6(&dir).as_deref(), Some("1"));
        assert!(migrated[1].contains("option dest 'vlan_guest'"));
        assert!(migrated[2].contains("option lan_access 'all'"));
        assert!(migrated[2].contains("option lan_access 'same_profile'"));
    }

    #[tokio::test]
    async fn migrations_after_the_record_run_in_order() {
        let dir = router(Some("2\n"), true);
        write(&dir, "etc/config/network", PPPOE_WAN);
        write(&dir, "etc/nftables.d/12-startwrt-sni-divert.nft", "");
        run(dir.path()).await.unwrap();
        assert_eq!(record(&dir), current());
        assert_eq!(wan_ipv6(&dir).as_deref(), Some("1"));
        assert!(dir
            .path()
            .join("etc/nftables.d/12-startwrt-sni-divert.nft")
            .exists());
    }

    #[tokio::test]
    async fn current_record_leaves_luci_pppoe_on_automatic() {
        let dir = router(Some(&current()), true);
        write(&dir, "etc/config/network", PPPOE_WAN);
        let before = read(&dir, "etc/config/network");
        run(dir.path()).await.unwrap();
        run(dir.path()).await.unwrap();
        assert_eq!(read(&dir, "etc/config/network"), before);
        assert_eq!(record(&dir), current());
    }

    #[tokio::test]
    async fn failed_migration_stays_pending() {
        let dir = router(Some("2\n"), true);
        std::fs::create_dir_all(dir.path().join("etc/config/network")).unwrap();
        assert!(run(dir.path()).await.is_err());
        assert_eq!(record(&dir), "2\n");
    }

    #[tokio::test]
    async fn invalid_record_runs_nothing() {
        let dir = router(Some("two\n"), true);
        write(&dir, "etc/config/network", PPPOE_WAN);
        assert!(run(dir.path()).await.is_err());
        assert_eq!(wan_ipv6(&dir), None);
    }

    #[tokio::test]
    async fn pppoe_migration_keeps_explicit_and_non_pppoe_settings() {
        for (network, expected) in [
            (format!("{PPPOE_WAN}\toption ipv6 'auto'\n"), Some("auto")),
            (format!("{PPPOE_WAN}\toption ipv6 '0'\n"), Some("0")),
            (
                "config interface 'wan'\n\toption device 'eth1'\n\toption proto 'dhcp'\n"
                    .to_string(),
                None,
            ),
            (
                "config interface 'lan'\n\toption proto 'pppoe'\n".to_string(),
                None,
            ),
        ] {
            let dir = router(Some("2\n"), true);
            write(&dir, "etc/config/network", &network);
            run(dir.path()).await.unwrap();
            assert_eq!(wan_ipv6(&dir).as_deref(), expected, "{network}");
            assert_eq!(record(&dir), current());
        }
    }

    #[tokio::test]
    async fn log_retention_expands_only_the_shipped_size() {
        let dir = router(None, true);
        write(&dir, "etc/config/system", SHIPPED_SYSTEM);
        expand_log_retention(dir.path()).await.unwrap();
        let system = read(&dir, "etc/config/system");
        assert!(system.contains("option log_size '32768'"), "{system}");
        assert!(system.contains("option log_buffer_size '512'"), "{system}");
        assert!(system.contains("option hostname 'router'"), "{system}");
        assert!(
            system.contains("option log_file '/etc/log/messages'"),
            "{system}"
        );

        for custom in [
            SHIPPED_SYSTEM.replace("'512'", "'2048'"),
            format!("{SHIPPED_SYSTEM}\toption log_buffer_size '128'\n"),
        ] {
            let dir = router(None, true);
            write(&dir, "etc/config/system", &custom);
            expand_log_retention(dir.path()).await.unwrap();
            assert_eq!(read(&dir, "etc/config/system"), custom);
        }
    }

    #[tokio::test]
    async fn logs_move_from_etc_into_var() {
        let dir = router(None, true);
        write(&dir, "etc/config/system", SHIPPED_SYSTEM);
        write(&dir, "etc/log/messages.old", "a\n");
        write(&dir, "etc/log/messages", "b\n");
        write(&dir, LEGACY_ACTIVITY_DB, "db");
        store_logs_under_var(dir.path()).await.unwrap();
        assert_eq!(read(&dir, "var/log/messages.old"), "a\n");
        assert_eq!(read(&dir, "var/log/messages"), "b\n");
        assert_eq!(read(&dir, "var/lib/startwrt/activity.db"), "db");
        assert!(!dir.path().join("etc/log").exists());
        assert!(!dir.path().join(LEGACY_ACTIVITY_DB).exists());
        let system = read(&dir, "etc/config/system");
        assert!(
            system.contains("option log_file '/var/log/messages'"),
            "{system}"
        );
        assert!(system.contains("option log_size '512'"), "{system}");
        assert!(system.contains("option hostname 'router'"), "{system}");

        store_logs_under_var(dir.path()).await.unwrap();
        assert_eq!(read(&dir, "var/log/messages"), "b\n");
        assert_eq!(read(&dir, "etc/config/system"), system);
    }

    #[tokio::test]
    async fn lines_logged_before_the_move_follow_the_log_already_in_var() {
        let dir = router(None, true);
        write(&dir, "etc/config/system", SHIPPED_SYSTEM);
        write(&dir, "var/log/messages", "history\n");
        write(&dir, "etc/log/messages", "boot\n");
        write(&dir, "var/lib/startwrt/activity.db", "new");
        write(&dir, LEGACY_ACTIVITY_DB, "old");
        store_logs_under_var(dir.path()).await.unwrap();
        assert_eq!(read(&dir, "var/log/messages"), "history\nboot\n");
        assert!(!dir.path().join("etc/log").exists());
        assert_eq!(read(&dir, "var/lib/startwrt/activity.db"), "new");
        assert_eq!(read(&dir, LEGACY_ACTIVITY_DB), "old");
    }

    #[tokio::test]
    async fn a_custom_log_file_stays_where_it_is() {
        let dir = router(None, true);
        let custom = SHIPPED_SYSTEM.replace("/etc/log/messages", "/mnt/usb/messages");
        write(&dir, "etc/config/system", &custom);
        store_logs_under_var(dir.path()).await.unwrap();
        assert_eq!(read(&dir, "etc/config/system"), custom);
    }

    #[tokio::test]
    async fn dhcp_pools_skip_the_foreign_server_probe() {
        let dir = router(Some("7\n"), true);
        write(&dir, "etc/config/dhcp", LEGACY_DHCP);
        run(dir.path()).await.unwrap();
        assert_eq!(record(&dir), current());
        let dhcp = read(&dir, "etc/config/dhcp");
        let section = |name: &str| {
            dhcp.split("\n\n")
                .find(|s| s.contains(&format!("config dhcp '{name}'")))
                .unwrap()
                .to_string()
        };
        assert!(section("lan").contains("option force '1'"));
        assert!(section("guest").contains("option force '1'"));
        assert!(!section("guest").contains("option force '0'"));
        assert!(!section("wan").contains("force"));
        assert!(!dhcp.split("\n\n").next().unwrap().contains("force"));
    }

    #[tokio::test]
    async fn restored_backup_without_record_migrates_on_next_boot() {
        let dir = router(Some(&current()), true);
        forget_unless_restored(
            dir.path(),
            "etc/config/network\netc/ssl/certs/startwrt-ca.pem\n",
        )
        .await
        .unwrap();
        assert!(!dir.path().join(RECORD).exists());

        write(&dir, RECORD, "1\n");
        forget_unless_restored(dir.path(), "etc/config/network\netc/startwrt/migrations\n")
            .await
            .unwrap();
        assert_eq!(record(&dir), "1\n");
    }
}
