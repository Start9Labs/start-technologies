//! One-time upgrade migrations, run at boot before `network` starts.
//!
//! `/etc/startwrt/migrations` records how many entries of [`MIGRATIONS`] this
//! router's configuration has been through. sysupgrade, the setup wizard's
//! Update flash and config backups carry it with the rest of the configuration.

use std::path::{Path, PathBuf};

use uciedit::openwrt::{InterfaceProto, NetworkInterface};
use uciedit::{dump_all, parse_all, Arena};

use crate::prelude::*;
use crate::wan::WAN_INTERFACE;

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
}

impl Migration {
    async fn apply(self, root: &Path) -> Result<(), Error> {
        match self {
            Migration::RemoveEtcNftablesRules => remove_etc_nftables_rules(root).await,
            Migration::StripUnreferencedWanZoneVpns => strip_unreferenced_wan_zone_vpns(root).await,
            Migration::PppoeWanIpv6 => pppoe_wan_ipv6(root).await,
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

#[cfg(test)]
mod tests {
    use super::*;

    const PPPOE_WAN: &str = "\
config interface 'wan'
\toption device 'eth1'
\toption proto 'pppoe'
\toption username 'user'
\toption password 'pass'
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

    fn current() -> String {
        format!("{}\n", MIGRATIONS.len())
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
            "config zone 'wan'\n\toption name 'wan'\n\toption input 'REJECT'\n\toption output 'ACCEPT'\n\toption forward 'REJECT'\n\tlist network 'wan'\n\tlist network 'wg_old'\n",
        );
        for migration in MIGRATIONS {
            migration.apply(dir.path()).await.unwrap();
        }
        let migrated = [
            read(&dir, "etc/config/network"),
            read(&dir, "etc/config/firewall"),
        ];
        for migration in MIGRATIONS {
            migration.apply(dir.path()).await.unwrap();
        }
        assert_eq!(read(&dir, "etc/config/network"), migrated[0]);
        assert_eq!(read(&dir, "etc/config/firewall"), migrated[1]);
        assert_eq!(wan_ipv6(&dir).as_deref(), Some("1"));
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
