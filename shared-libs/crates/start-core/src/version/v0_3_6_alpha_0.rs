use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::Path;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use const_format::formatcp;
use ed25519_dalek::SigningKey;
use exver::{PreReleaseSegment, VersionRange};
use imbl_value::{InternedString, json};
use itertools::Itertools;
use openssl::pkey::PKey;
use openssl::x509::X509;
use sqlx::postgres::PgConnectOptions;
use sqlx::{PgPool, Row};
use tokio::process::Command;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_3_5_2};
use crate::account::AccountInfo;
use crate::backup::target::cifs::CifsTargets;
use crate::context::RpcContext;
use crate::disk::mount::filesystem::cifs::Cifs;
use crate::disk::mount::util::unmount;
use crate::hostname::{ServerHostname, ServerHostnameInfo};
use crate::net::forward::AvailablePorts;
use crate::net::keys::KeyStore;
use crate::notifications::{NotificationLevel, Notifications, notify};
use crate::prelude::*;
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;
use crate::s9pk::v2::pack::CONTAINER_TOOL;
use crate::ssh::{SshKeys, SshPubKey};
use crate::util::Invoke;
use crate::util::io::write_file_atomic;
use crate::util::serde::Pem;
use crate::volume::PKG_VOLUME_DIR;
use crate::{DATA_DIR, PACKAGE_DATA, PackageId, ReplayId};

lazy_static::lazy_static! {
    static ref V0_3_6_alpha_0: exver::Version = exver::Version::new(
        [0, 3, 6],
        [PreReleaseSegment::String("alpha".into()), 0.into()]
    );
}

/// All pre-0.4.0 StartOS images were initialized with the en_GB.UTF-8 locale.
/// The current trixie image does not ship it.  Without it PostgreSQL starts
/// but refuses connections, breaking the migration.
async fn ensure_en_gb_locale() -> Result<(), Error> {
    Command::new("localedef")
        .arg("-i")
        .arg("en_GB")
        .arg("-c")
        .arg("-f")
        .arg("UTF-8")
        .arg("en_GB.UTF-8")
        .invoke(crate::ErrorKind::Database)
        .await?;
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn init_postgres(datadir: impl AsRef<Path>) -> Result<PgPool, Error> {
    let db_dir = datadir.as_ref().join("main/postgresql");
    if tokio::process::Command::new("mountpoint")
        .arg("/var/lib/postgresql")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await?
        .success()
    {
        unmount("/var/lib/postgresql", true).await?;
    }
    let exists = tokio::fs::metadata(&db_dir).await.is_ok();
    if !exists {
        Command::new("cp")
            .arg("-ra")
            .arg("/var/lib/postgresql")
            .arg(&db_dir)
            .invoke(crate::ErrorKind::Filesystem)
            .await?;
    }
    Command::new("chown")
        .arg("-R")
        .arg("postgres:postgres")
        .arg(&db_dir)
        .invoke(crate::ErrorKind::Database)
        .await?;

    let mut pg_paths = tokio::fs::read_dir("/usr/lib/postgresql").await?;
    let mut pg_version = None;
    while let Some(pg_path) = pg_paths.next_entry().await? {
        let pg_path_version = pg_path
            .file_name()
            .to_str()
            .map(|v| v.parse())
            .transpose()?
            .unwrap_or(0);
        if pg_path_version > pg_version.unwrap_or(0) {
            pg_version = Some(pg_path_version)
        }
    }
    let pg_version = pg_version.ok_or_else(|| {
        Error::new(
            eyre!("could not determine postgresql version"),
            crate::ErrorKind::Database,
        )
    })?;

    crate::disk::mount::util::bind(&db_dir, "/var/lib/postgresql", false).await?;

    // The cluster may have been created with a locale not present on the
    // current image (e.g. en_GB.UTF-8 on a server that predates the trixie
    // image).  Detect and generate it before starting PostgreSQL, otherwise
    // PG will start but refuse connections.
    ensure_en_gb_locale().await?;

    Command::new("systemctl")
        .arg("start")
        .arg(format!("postgresql@{pg_version}-main.service"))
        .invoke(crate::ErrorKind::Database)
        .await?;
    if !exists {
        Command::new("sudo")
            .arg("-u")
            .arg("postgres")
            .arg("createuser")
            .arg("root")
            .invoke(crate::ErrorKind::Database)
            .await?;
        Command::new("sudo")
            .arg("-u")
            .arg("postgres")
            .arg("createdb")
            .arg("secrets")
            .arg("-O")
            .arg("root")
            .invoke(crate::ErrorKind::Database)
            .await?;
    }

    let secret_store = if let Ok(s) = PgPool::connect_with(
        PgConnectOptions::new()
            .database("secrets")
            .username("root")
            .port(5432)
            .socket("/var/run/postgresql"),
    )
    .await
    {
        s
    } else {
        PgPool::connect_with(
            PgConnectOptions::new()
                .database("secrets")
                .username("root")
                .port(5433)
                .socket("/var/run/postgresql"),
        )
        .await
        .with_kind(ErrorKind::Database)?
    };
    Ok(secret_store)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_3_5_2::Version;
    type PreUpRes = (
        AccountInfo,
        SshKeys,
        CifsTargets,
        BTreeMap<(String, String), [u8; 64]>,
    );
    fn semver(self) -> exver::Version {
        V0_3_6_alpha_0.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        // Contingency for customers with corrupted PostgreSQL databases:
        // if this sentinel file exists, skip the database entirely and
        // regenerate fresh account data.  Tor keys, SSH keys, and CIFS
        // backup targets will be lost.
        if tokio::fs::metadata("/home/start9/PGDB_DO_NOT_MIGRATE")
            .await
            .is_ok()
        {
            tracing::warn!(
                "Found /home/start9/PGDB_DO_NOT_MIGRATE — \
                 skipping PostgreSQL migration, generating fresh account data"
            );
            let account = AccountInfo::new("embassy", std::time::SystemTime::now(), None)?;
            return Ok((
                account,
                SshKeys::new(),
                CifsTargets::default(),
                BTreeMap::new(),
            ));
        }

        let pg = init_postgres(DATA_DIR).await?;
        let account = previous_account_info(&pg).await?;

        let ssh_keys = previous_ssh_keys(&pg).await?;

        let cifs = previous_cifs(&pg).await?;

        let tor_keys = previous_tor_keys(&pg).await?;

        Command::new("systemctl")
            .arg("stop")
            .arg("postgresql@*.service")
            .invoke(crate::ErrorKind::Database)
            .await?;

        Ok((account, ssh_keys, cifs, tor_keys))
    }
    fn up(
        self,
        db: &mut Value,
        (account, ssh_keys, cifs, tor_keys): Self::PreUpRes,
    ) -> Result<Value, Error> {
        let prev_package_data = db["package-data"].clone();

        let wifi = json!({
            "interface": db["server-info"]["wifi"]["interface"],
            "ssids": db["server-info"]["wifi"]["ssids"],
            "selected": db["server-info"]["wifi"]["selected"],
            "lastRegion": db["server-info"]["wifi"]["last-region"],
        });

        let status_info = json!({
            "backupProgress": db["server-info"]["status-info"]["backup-progress"],
            "updated": db["server-info"]["status-info"]["updated"],
            "updateProgress": db["server-info"]["status-info"]["update-progress"],
            "shuttingDown": db["server-info"]["status-info"]["shutting-down"],
            "restarting": db["server-info"]["status-info"]["restarting"],
        });
        let tor_address: String = from_value(db["server-info"]["tor-address"].clone())?;
        let onion_address = tor_address
            .replace("https://", "")
            .replace("http://", "")
            .replace(".onion/", "");
        let server_info = {
            let mut server_info = json!({
                "arch": db["server-info"]["arch"],
                "platform": db["server-info"]["platform"],
                "id": db["server-info"]["id"],
                "hostname": db["server-info"]["hostname"],
                "version": db["server-info"]["version"],
                "versionCompat": db["server-info"]["eos-version-compat"],
                "lastBackup": db["server-info"]["last-backup"],
                "lanAddress": db["server-info"]["lan-address"],
            });

            server_info["postInitMigrationTodos"] = json!({});
            // Maybe we do this like the Public::init does
            server_info["torAddress"] = json!(&tor_address);
            server_info["onionAddress"] = json!(&onion_address);
            server_info["networkInterfaces"] = json!({});
            server_info["statusInfo"] = status_info;
            server_info["wifi"] = wifi;
            server_info["unreadNotificationCount"] =
                db["server-info"]["unread-notification-count"].clone();
            server_info["pubkey"] = db["server-info"]["pubkey"].clone();
            server_info["caFingerprint"] = db["server-info"]["ca-fingerprint"].clone();
            server_info["ntpSynced"] = db["server-info"]["ntp-synced"].clone();
            server_info["zram"] = db["server-info"]["zram"].clone();
            server_info["governor"] = db["server-info"]["governor"].clone();
            // This one should always be empty, doesn't exist in the previous. And the smtp is all single word key
            server_info["smtp"] = db["server-info"]["smtp"].clone();
            server_info
        };

        let public = json!({
            "serverInfo": server_info,
            "packageData": json!({}),
            "ui": db["ui"],
        });

        let keystore = KeyStore::new(&account)?;

        let private = {
            let mut value = json!({});
            value["keyStore"] = to_value(&keystore)?;
            // Preserve tor onion keys so later migrations (v0_4_0_alpha_20) can
            // include them in onion-migration.json for the tor service.
            // Always write torMigration (even if empty) so that
            // v0_4_0_alpha_20 takes the pre-built path and doesn't fall
            // back to looking up keys in the onion store.
            let mut onion_map: Value = json!({});
            let mut tor_migration = imbl::Vector::<Value>::new();
            if !tor_keys.is_empty() {
                let onion_obj = onion_map.as_object_mut().unwrap();
                for ((package_id, host_id), key_bytes) in &tor_keys {
                    let onion_addr = onion_address_from_key(key_bytes);
                    let encoded_key =
                        base64::Engine::encode(&crate::util::serde::BASE64, key_bytes);
                    onion_obj.insert(
                        onion_addr.as_str().into(),
                        Value::String(encoded_key.clone().into()),
                    );
                    tor_migration.push_back(json!({
                        "hostname": &onion_addr,
                        "packageId": package_id,
                        "hostId": host_id,
                        "key": &encoded_key,
                    }));
                }
                value["keyStore"]["onion"] = onion_map;
            }
            value["torMigration"] = Value::Array(tor_migration);
            value["password"] = to_value(&account.password)?;
            value["compatS9pkKey"] =
                to_value(&crate::db::model::private::generate_developer_key())?;
            value["sshPrivkey"] = to_value(Pem::new_ref(&account.ssh_key))?;
            value["sshPubkeys"] = to_value(&ssh_keys)?;
            value["availablePorts"] = to_value(&AvailablePorts::new())?;
            value["sessions"] = json!({});
            value["notifications"] = to_value(&Notifications::new())?;
            value["cifs"] = to_value(&cifs)?;
            value["packageStores"] = json!({});
            value
        };
        let next: Value = json!({
            "public": public,
            "private": private,
        });

        *db = next;

        Ok(prev_package_data)
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        Err(Error::new(
            eyre!("downgrades prohibited"),
            ErrorKind::InvalidRequest,
        ))
    }

    #[instrument(skip(self, ctx))]
    /// MUST be idempotent, and is run after *all* db migrations
    async fn post_up(self, ctx: &RpcContext, input: Value) -> Result<(), Error> {
        let path = Path::new(formatcp!("{PACKAGE_DATA}/archive/"));
        let metadata = tokio::fs::metadata(path).await;
        if metadata.is_err() {
            // Treat non-existent archive directory as empty
            return Ok(());
        }
        if !metadata.unwrap().is_dir() {
            return Err(Error::new(
                eyre!(
                    "expected path ({}) to be a directory",
                    path.to_string_lossy()
                ),
                ErrorKind::Filesystem,
            ));
        }

        if tokio::fs::metadata("/media/startos/data/package-data/volumes/nostr")
            .await
            .is_ok()
        {
            tokio::fs::rename(
                "/media/startos/data/package-data/volumes/nostr",
                "/media/startos/data/package-data/volumes/nostr-rs-relay",
            )
            .await?;
        }

        if tokio::fs::metadata("/media/startos/data/package-data/volumes/ghost")
            .await
            .is_ok()
        {
            tokio::fs::rename(
                "/media/startos/data/package-data/volumes/ghost",
                "/media/startos/data/package-data/volumes/ghost-legacy",
            )
            .await?;
        }

        if tokio::fs::metadata("/media/startos/data/package-data/volumes/synapse")
            .await
            .is_ok()
        {
            tokio::fs::rename(
                "/media/startos/data/package-data/volumes/synapse",
                "/media/startos/data/package-data/volumes/synapse-legacy",
            )
            .await?;
        }

        if tokio::fs::metadata("/media/startos/data/package-data/volumes/monerod")
            .await
            .is_ok()
        {
            tokio::fs::rename(
                "/media/startos/data/package-data/volumes/monerod",
                "/media/startos/data/package-data/volumes/monerod-legacy",
            )
            .await?;
        }

        if tokio::fs::metadata("/media/startos/data/package-data/volumes/fedimintd")
            .await
            .is_ok()
        {
            tokio::fs::rename(
                "/media/startos/data/package-data/volumes/fedimintd",
                "/media/startos/data/package-data/volumes/fedimint-guardian",
            )
            .await?;
        }

        // Load bundled migration images (start9/compat, start9/utils,
        // tonistiigi/binfmt) so the v1->v2 s9pk conversion doesn't need
        // internet access.
        let migration_images_dir = Path::new("/usr/lib/startos/migration-images");
        if let Ok(mut entries) = tokio::fs::read_dir(migration_images_dir).await {
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.extension() == Some(OsStr::new("tar")) {
                    tracing::info!("Loading migration image: {}", path.display());
                    Command::new(*CONTAINER_TOOL)
                        .arg("load")
                        .arg("-i")
                        .arg(&path)
                        .invoke(crate::ErrorKind::Docker)
                        .await?;
                }
            }
        }

        let mut failures = BTreeMap::new();

        // Should be the name of the package
        let current_package: std::sync::Arc<tokio::sync::watch::Sender<Option<PackageId>>> =
            std::sync::Arc::new(tokio::sync::watch::channel(None).0);
        let progress_logger = {
            let current_package = current_package.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
                interval.tick().await; // skip immediate first tick
                loop {
                    interval.tick().await;
                    if let Some(ref id) = *current_package.borrow() {
                        tracing::info!(
                            "{}",
                            t!("migration.migrating-package", package = id.to_string())
                        );
                    }
                }
            })
        };
        let mut paths = tokio::fs::read_dir(path).await?;
        while let Some(path) = paths.next_entry().await? {
            let Ok(id) = path.file_name().to_string_lossy().parse::<PackageId>() else {
                continue;
            };
            let path = path.path();
            if !path.is_dir() {
                continue;
            }
            // Should be the version of the package
            let mut paths = tokio::fs::read_dir(path).await?;
            while let Some(path) = paths.next_entry().await? {
                let path = path.path();
                if !path.is_dir() {
                    continue;
                }

                // Should be s9pk
                let mut paths = tokio::fs::read_dir(path).await?;
                while let Some(path) = paths.next_entry().await? {
                    let path = path.path();
                    if path.extension() != Some(OsStr::new("s9pk")) {
                        continue;
                    }

                    let configured = if !input.is_null() {
                        let Some(configured) = input
                            .get(&*id)
                            .and_then(|pde| pde.get("installed"))
                            .and_then(|i| i.get("status"))
                            .and_then(|s| s.get("configured"))
                            .and_then(|c| c.as_bool())
                        else {
                            continue;
                        };
                        configured
                    } else {
                        false
                    };

                    tracing::info!(
                        "{}",
                        t!("migration.migrating-package", package = id.to_string())
                    );
                    current_package.send_replace(Some(id.clone()));

                    // Write the data version from the old DB to disk so the
                    // install process detects existing data and uses
                    // InitKind::Update instead of InitKind::Install. The old
                    // DB stores versions in emver format (e.g. `0.21.1.0`),
                    // but callers parse `.version` as `exver::ExtendedVersion`
                    // (e.g. `0.21.1:0`), so convert before writing — and also
                    // apply the same package-specific flavor/prerelease/id
                    // rewrites that the v1→v2 s9pk conversion in
                    // `s9pk::v2::compat` applies, so the on-disk version and
                    // volume path match what the install will look up.
                    let installed_manifest = input
                        .get(&*id)
                        .and_then(|pde| pde.get("installed"))
                        .and_then(|i| i.get("manifest"));
                    if let Some(emver_str) = installed_manifest
                        .and_then(|m| m.get("version"))
                        .and_then(|v| v.as_str())
                    {
                        if let Ok(emver) = exver::emver::Version::from_str(emver_str) {
                            let title = installed_manifest
                                .and_then(|m| m.get("title"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let mut version = exver::ExtendedVersion::from(emver);
                            if &*id == "bitcoind" && title.to_ascii_lowercase().contains("knots") {
                                version = version.with_flavor("knots");
                            } else if &*id == "lnd"
                                || &*id == "ride-the-lightning"
                                || &*id == "datum"
                            {
                                version =
                                    version.map_upstream(|v| v.with_prerelease(["beta".into()]));
                            } else if &*id == "lightning-terminal" || &*id == "robosats" {
                                version =
                                    version.map_upstream(|v| v.with_prerelease(["alpha".into()]));
                            }
                            // The rename pass at the top of post_up has
                            // already moved the volume dirs to their new
                            // names, so we must write under the new id.
                            let new_package_id: &str = match &*id {
                                "nostr" => "nostr-rs-relay",
                                "ghost" => "ghost-legacy",
                                "synapse" => "synapse-legacy",
                                "monerod" => "monerod-legacy",
                                "fedimintd" => "fedimint-guardian",
                                other => other,
                            };
                            let version_path = Path::new(DATA_DIR)
                                .join(PKG_VOLUME_DIR)
                                .join(new_package_id)
                                .join("data")
                                .join(".version");
                            write_file_atomic(&version_path, version.to_string().as_bytes())
                                .await?;
                        }
                    }

                    if let Err(e) = async {
                        let package_s9pk = tokio::fs::File::open(path).await?;
                        let file = MultiCursorFile::open(&package_s9pk).await?;

                        let key = ctx.db.peek().await.into_private().into_developer_key();
                        ctx.services
                            .install(
                                ctx.clone(),
                                || crate::s9pk::load(file.clone(), || Ok(key.de()?.0), None),
                                None,
                                None::<crate::util::Never>,
                                None,
                            )
                            .await?
                            .await?
                            .await?;

                        ctx.db
                            .mutate(|db| {
                                let package = db
                                    .as_public_mut()
                                    .as_package_data_mut()
                                    .as_idx_mut(&id)
                                    .or_not_found(&id)?;
                                if configured {
                                    package
                                        .as_tasks_mut()
                                        .remove(&ReplayId::from("needs-config"))?;
                                }
                                Ok(())
                            })
                            .await
                            .result?;

                        Ok::<_, Error>(())
                    }
                    .await
                    {
                        failures.insert(
                            id.clone(),
                            input
                                .get(&*id)
                                .and_then(|pde| pde.get("installed"))
                                .and_then(|i| i.get("manifest"))
                                .and_then(|m| m.get("title"))
                                .and_then(|v| v.as_str()),
                        );
                        tracing::error!("Error reinstalling {id}: {e}");
                        tracing::debug!("{e:?}");
                    }
                }
            }
        }

        if !failures.is_empty() {
            ctx.db
                .mutate(|db| {
                    let services = failures
                        .iter()
                        .map(|(id, title)| title.as_ref().copied().unwrap_or(id))
                        .join(", ");
                    notify(
                        db,
                        None,
                        NotificationLevel::Error,
                        t!("migration.services-failed-title").to_string(),
                        t!("migration.services-failed-message", services = services).to_string(),
                        (),
                    )
                })
                .await
                .result?;
        }

        progress_logger.abort();
        Ok(())
    }
}

#[tracing::instrument(skip_all)]
async fn previous_cifs(pg: &sqlx::Pool<sqlx::Postgres>) -> Result<CifsTargets, Error> {
    let cifs = sqlx::query(r#"SELECT * FROM cifs_shares"#)
        .fetch_all(pg)
        .await
        .with_kind(ErrorKind::Database)?
        .into_iter()
        .map(|row| {
            let id: i32 = row.try_get("id").with_kind(ErrorKind::Database)?;
            Ok::<_, Error>((
                id,
                Cifs {
                    hostname: row
                        .try_get("hostname")
                        .with_ctx(|_| (ErrorKind::Database, "hostname"))?,
                    path: row
                        .try_get::<String, _>("path")
                        .with_ctx(|_| (ErrorKind::Database, "path"))?
                        .into(),
                    username: row
                        .try_get("username")
                        .with_ctx(|_| (ErrorKind::Database, "username"))?,
                    password: row
                        .try_get("password")
                        .with_ctx(|_| (ErrorKind::Database, "password"))?,
                },
            ))
        })
        .fold(Ok::<_, Error>(CifsTargets::default()), |cifs, data| {
            let mut cifs = cifs?;
            let (id, cif_value) = data?;
            cifs.0.insert(id as u32, cif_value);
            Ok(cifs)
        })?;
    Ok(cifs)
}

#[tracing::instrument(skip_all)]
async fn previous_account_info(pg: &sqlx::Pool<sqlx::Postgres>) -> Result<AccountInfo, Error> {
    let account_query = sqlx::query(r#"SELECT * FROM account"#)
        .fetch_one(pg)
        .await
        .with_kind(ErrorKind::Database)?;
    let account = {
        AccountInfo {
            password: account_query
                .try_get("password")
                .with_ctx(|_| (ErrorKind::Database, "password"))?,
            server_id: account_query
                .try_get("server_id")
                .with_ctx(|_| (ErrorKind::Database, "server_id"))?,
            hostname: ServerHostnameInfo::from_hostname(ServerHostname::new(
                account_query
                    .try_get::<String, _>("hostname")
                    .with_ctx(|_| (ErrorKind::Database, "hostname"))?
                    .into(),
            )?),
            root_ca_key: PKey::private_key_from_pem(
                &account_query
                    .try_get::<String, _>("root_ca_key_pem")
                    .with_ctx(|_| (ErrorKind::Database, "root_ca_key_pem"))?
                    .as_bytes(),
            )
            .with_ctx(|_| (ErrorKind::Database, "private_key_from_pem"))?,
            root_ca_cert: X509::from_pem(
                account_query
                    .try_get::<String, _>("root_ca_cert_pem")
                    .with_ctx(|_| (ErrorKind::Database, "root_ca_cert_pem"))?
                    .as_bytes(),
            )
            .with_ctx(|_| (ErrorKind::Database, "X509::from_pem"))?,
            developer_key: SigningKey::generate(&mut crate::util::crypto::os_rng()),
            ssh_key: ssh_key::PrivateKey::random(
                &mut crate::util::crypto::os_rng(),
                ssh_key::Algorithm::Ed25519,
            )
            .with_ctx(|_| (ErrorKind::Database, "X509::ssh_key::PrivateKey::random"))?,
        }
    };
    Ok(account)
}
#[tracing::instrument(skip_all)]
async fn previous_ssh_keys(pg: &sqlx::Pool<sqlx::Postgres>) -> Result<SshKeys, Error> {
    let ssh_query = sqlx::query(r#"SELECT * FROM ssh_keys"#)
        .fetch_all(pg)
        .await
        .with_kind(ErrorKind::Database)?;
    let ssh_keys: SshKeys = {
        let keys = ssh_query.into_iter().fold(
            Ok::<_, Error>(BTreeMap::<InternedString, WithTimeData<SshPubKey>>::new()),
            |ssh_keys, row| {
                let mut ssh_keys = ssh_keys?;
                let time = row
                    .try_get::<String, _>("created_at")
                    .with_kind(ErrorKind::Database)
                    .and_then(|x| x.parse::<DateTime<Utc>>().with_kind(ErrorKind::Database))
                    .with_ctx(|_| (ErrorKind::Database, "openssh_pubkey::created_at"))?;
                let value: SshPubKey = row
                    .try_get::<String, _>("openssh_pubkey")
                    .with_kind(ErrorKind::Database)
                    .and_then(|x| x.parse().map(SshPubKey).with_kind(ErrorKind::Database))
                    .with_ctx(|_| (ErrorKind::Database, "openssh_pubkey"))?;
                let data = WithTimeData {
                    created_at: time,
                    updated_at: time,
                    value,
                };
                let fingerprint = row
                    .try_get::<String, _>("fingerprint")
                    .with_ctx(|_| (ErrorKind::Database, "fingerprint"))?;
                ssh_keys.insert(fingerprint.into(), data);
                Ok(ssh_keys)
            },
        )?;
        SshKeys::from(keys)
    };
    Ok(ssh_keys)
}

/// Returns deduplicated map of `(package_id, host_id) -> expanded_key`.
/// Server key uses `("start-os", "admin")` — the StartOS UI's service-model
/// identity (see `PackageId::start_os` / `HostId::admin`).
/// When the same (package, interface) exists in both the `network_keys` and
/// `tor` tables, the `tor` table entry wins because it contains the actual
/// expanded key that was used by tor.
#[tracing::instrument(skip_all)]
async fn previous_tor_keys(
    pg: &sqlx::Pool<sqlx::Postgres>,
) -> Result<BTreeMap<(String, String), [u8; 64]>, Error> {
    let mut keys = BTreeMap::new();

    // Server tor key from the account table.
    // Older installs have tor_key (64 bytes). Newer installs (post-NetworkKeys migration)
    // made tor_key nullable and use network_key (32 bytes, needs expansion) instead.
    let row = sqlx::query(r#"SELECT tor_key, network_key FROM account"#)
        .fetch_one(pg)
        .await
        .with_kind(ErrorKind::Database)?;
    if let Ok(tor_key) = row.try_get::<Vec<u8>, _>("tor_key") {
        if let Ok(key) = <[u8; 64]>::try_from(tor_key) {
            keys.insert(("start-os".to_owned(), "admin".to_owned()), key);
        }
    } else if let Ok(net_key) = row.try_get::<Vec<u8>, _>("network_key") {
        if let Ok(seed) = <[u8; 32]>::try_from(net_key) {
            keys.insert(
                ("start-os".to_owned(), "admin".to_owned()),
                crate::util::crypto::ed25519_expand_key(&seed),
            );
        }
    }

    // Package tor keys from the network_keys table (32-byte keys that need expansion)
    if let Ok(rows) = sqlx::query(r#"SELECT package, interface, key FROM network_keys"#)
        .fetch_all(pg)
        .await
    {
        for row in rows {
            let Ok(package) = row.try_get::<String, _>("package") else {
                continue;
            };
            let Ok(interface) = row.try_get::<String, _>("interface") else {
                continue;
            };
            let Ok(key_bytes) = row.try_get::<Vec<u8>, _>("key") else {
                continue;
            };
            if let Ok(seed) = <[u8; 32]>::try_from(key_bytes) {
                keys.insert(
                    (package, interface),
                    crate::util::crypto::ed25519_expand_key(&seed),
                );
            }
        }
    }

    // Package tor keys from the tor table (already 64-byte expanded keys).
    // These overwrite network_keys entries for the same (package, interface)
    // because the tor table has the actual expanded key used by tor.
    if let Ok(rows) = sqlx::query(r#"SELECT package, interface, key FROM tor"#)
        .fetch_all(pg)
        .await
    {
        for row in rows {
            let Ok(package) = row.try_get::<String, _>("package") else {
                continue;
            };
            let Ok(interface) = row.try_get::<String, _>("interface") else {
                continue;
            };
            let Ok(key_bytes) = row.try_get::<Vec<u8>, _>("key") else {
                continue;
            };
            if let Ok(key) = <[u8; 64]>::try_from(key_bytes) {
                keys.insert((package, interface), key);
            }
        }
    }

    Ok(keys)
}

/// Derive the tor v3 onion address (without .onion suffix) from a 64-byte
/// expanded ed25519 secret key.
fn onion_address_from_key(expanded_key: &[u8; 64]) -> String {
    use sha3::Digest;

    // Derive public key from expanded secret key using ed25519-dalek v1
    let esk =
        ed25519_dalek_v1::ExpandedSecretKey::from_bytes(expanded_key).expect("invalid tor key");
    let pk = ed25519_dalek_v1::PublicKey::from(&esk);
    let pk_bytes = pk.to_bytes();

    // Compute onion v3 address: base32(pubkey || checksum || version)
    // checksum = SHA3-256(".onion checksum" || pubkey || version)[0..2]
    let mut hasher = sha3::Sha3_256::new();
    hasher.update(b".onion checksum");
    hasher.update(&pk_bytes);
    hasher.update(b"\x03");
    let hash = hasher.finalize();

    let mut raw = [0u8; 35];
    raw[..32].copy_from_slice(&pk_bytes);
    raw[32] = hash[0]; // checksum byte 0
    raw[33] = hash[1]; // checksum byte 1
    raw[34] = 0x03; // version

    base32::encode(base32::Alphabet::Rfc4648 { padding: false }, &raw).to_ascii_lowercase()
}
