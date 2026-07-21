use std::path::Path;

use exver::{PreReleaseSegment, VersionRange};
use imbl_value::json;
use reqwest::Url;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_0_alpha_19};
use crate::context::RpcContext;
use crate::prelude::*;
use crate::s9pk::merkle_archive::source::multi_cursor_file::MultiCursorFile;

lazy_static::lazy_static! {
    static ref V0_4_0_alpha_20: exver::Version = exver::Version::new(
        [0, 4, 0],
        [PreReleaseSegment::String("alpha".into()), 20.into()]
    );
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_0_alpha_19::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_0_alpha_20.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    #[instrument(skip_all)]
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        // Use the pre-built torMigration data from v0_3_6_alpha_0 if available.
        // This contains all (hostname, packageId, hostId, key) entries with keys
        // already resolved, avoiding the issue where packageData is empty during
        // migration (packages aren't reinstalled until post_up).
        let migration_data = if let Some(tor_migration) = db
            .get("private")
            .and_then(|p| p.get("torMigration"))
            .and_then(|t| t.as_array())
        {
            let mut addresses = tor_migration.clone();
            for entry in addresses.iter_mut() {
                normalize_legacy_service_identity(entry);
            }
            json!({
                "addresses": addresses,
            })
        } else {
            // Fallback for fresh installs or installs that didn't go through
            // v0_3_6_alpha_0 with the torMigration field.
            let onion_store = db
                .get("private")
                .and_then(|p| p.get("keyStore"))
                .and_then(|k| k.get("onion"))
                .cloned()
                .unwrap_or(Value::Object(Default::default()));

            let mut addresses = imbl::Vector::<Value>::new();

            // Extract OS host onion addresses
            if let Some(onions) = db
                .get("public")
                .and_then(|p| p.get("serverInfo"))
                .and_then(|s| s.get("network"))
                .and_then(|n| n.get("host"))
                .and_then(|h| h.get("onions"))
                .and_then(|o| o.as_array())
            {
                for onion in onions {
                    if let Some(hostname) = onion.as_str() {
                        let key = onion_store
                            .get(hostname)
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                Error::new(
                                    eyre!("missing tor key for onion address {hostname}"),
                                    ErrorKind::Database,
                                )
                            })?;
                        addresses.push_back(json!({
                            "hostname": hostname,
                            "packageId": "start-os",
                            "hostId": "admin",
                            "key": key,
                        }));
                    }
                }
            }

            // Extract package host onion addresses
            if let Some(packages) = db
                .get("public")
                .and_then(|p| p.get("packageData"))
                .and_then(|p| p.as_object())
            {
                for (package_id, package) in packages.iter() {
                    if let Some(hosts) = package.get("hosts").and_then(|h| h.as_object()) {
                        for (host_id, host) in hosts.iter() {
                            if let Some(onions) = host.get("onions").and_then(|o| o.as_array()) {
                                for onion in onions {
                                    if let Some(hostname) = onion.as_str() {
                                        let key = onion_store
                                            .get(hostname)
                                            .and_then(|v| v.as_str())
                                            .ok_or_else(|| {
                                                Error::new(
                                                    eyre!(
                                                        "missing tor key for onion address {hostname}"
                                                    ),
                                                    ErrorKind::Database,
                                                )
                                            })?;
                                        addresses.push_back(json!({
                                            "hostname": hostname,
                                            "packageId": &**package_id,
                                            "hostId": &**host_id,
                                            "key": key,
                                        }));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            json!({
                "addresses": addresses,
            })
        };

        // Clean up torMigration from private
        if let Some(private) = db.get_mut("private").and_then(|p| p.as_object_mut()) {
            private.remove("torMigration");
        }

        // Remove onions and tor-related fields from server host
        if let Some(host) = db
            .get_mut("public")
            .and_then(|p| p.get_mut("serverInfo"))
            .and_then(|s| s.get_mut("network"))
            .and_then(|n| n.get_mut("host"))
            .and_then(|h| h.as_object_mut())
        {
            host.remove("onions");
        }

        // Remove onions from all package hosts
        if let Some(packages) = db
            .get_mut("public")
            .and_then(|p| p.get_mut("packageData"))
            .and_then(|p| p.as_object_mut())
        {
            for (_, package) in packages.iter_mut() {
                if let Some(hosts) = package.get_mut("hosts").and_then(|h| h.as_object_mut()) {
                    for (_, host) in hosts.iter_mut() {
                        if let Some(host_obj) = host.as_object_mut() {
                            host_obj.remove("onions");
                        }
                    }
                }
            }
        }

        // Remove onion store from private keyStore
        if let Some(key_store) = db
            .get_mut("private")
            .and_then(|p| p.get_mut("keyStore"))
            .and_then(|k| k.as_object_mut())
        {
            key_store.remove("onion");
        }

        // Migrate server host: remove hostnameInfo, add addresses to bindings, clean net
        migrate_host(
            db.get_mut("public")
                .and_then(|p| p.get_mut("serverInfo"))
                .and_then(|s| s.get_mut("network"))
                .and_then(|n| n.get_mut("host")),
        );

        // Migrate all package hosts
        if let Some(packages) = db
            .get_mut("public")
            .and_then(|p| p.get_mut("packageData"))
            .and_then(|p| p.as_object_mut())
        {
            for (_, package) in packages.iter_mut() {
                if let Some(hosts) = package.get_mut("hosts").and_then(|h| h.as_object_mut()) {
                    for (_, host) in hosts.iter_mut() {
                        migrate_host(Some(host));
                    }
                }
            }
        }

        // Migrate availablePorts from IdPool format to BTreeMap<u16, bool>
        // Rebuild from actual assigned ports in all bindings
        migrate_available_ports(db);

        // Migrate SMTP: rename server->host, login->username, add security field
        migrate_smtp(db);

        // Delete ui.name (moved to serverInfo.name)
        if let Some(ui) = db
            .get_mut("public")
            .and_then(|p| p.get_mut("ui"))
            .and_then(|u| u.as_object_mut())
        {
            ui.remove("name");
        }

        // Generate serverInfo.name from serverInfo.hostname
        if let Some(hostname) = db
            .get("public")
            .and_then(|p| p.get("serverInfo"))
            .and_then(|s| s.get("hostname"))
            .and_then(|h| h.as_str())
            .map(|s| s.to_owned())
        {
            let name = denormalize_hostname(&hostname);
            if let Some(server_info) = db
                .get_mut("public")
                .and_then(|p| p.get_mut("serverInfo"))
                .and_then(|s| s.as_object_mut())
            {
                server_info.insert("name".into(), Value::String(name.into()));
            }
        }

        Ok(migration_data)
    }

    #[instrument(skip_all)]
    async fn post_up(self, ctx: &RpcContext, input: Value) -> Result<(), Error> {
        let path = Path::new(
            "/media/startos/data/package-data/volumes/tor/data/startos/onion-migration.json",
        );

        let json = serde_json::to_string(&input).with_kind(ErrorKind::Serialization)?;

        crate::util::io::write_file_atomic(path, json).await?;

        // Sideload the bundled tor s9pk
        let s9pk_path_str = format!("/usr/lib/startos/tor_{}.s9pk", crate::ARCH);
        let s9pk_path = Path::new(&s9pk_path_str);
        if tokio::fs::metadata(s9pk_path).await.is_ok() {
            if let Err(e) = async {
                let package_s9pk = tokio::fs::File::open(s9pk_path).await?;
                let file = MultiCursorFile::open(&package_s9pk).await?;

                let key = ctx.db.peek().await.into_private().into_developer_key();
                let registry_url =
                    Url::parse("https://registry.start9.com/").with_kind(ErrorKind::ParseUrl)?;

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

                // Set the marketplace URL on the installed tor package
                let tor_id = "tor".parse::<crate::PackageId>()?;
                ctx.db
                    .mutate(|db| {
                        if let Some(pkg) =
                            db.as_public_mut().as_package_data_mut().as_idx_mut(&tor_id)
                        {
                            pkg.as_registry_mut().ser(&Some(registry_url))?;
                        }
                        Ok(())
                    })
                    .await
                    .result?;

                Ok::<_, Error>(())
            }
            .await
            {
                tracing::error!("Error installing tor package: {e}");
                tracing::debug!("{e:?}");
            }
        }

        Ok(())
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        Ok(())
    }
}

/// The server UI's onion entries were keyed by the legacy `STARTOS` sentinel
/// (with host id `STARTOS` or `startos-ui`) before the StartOS UI became an
/// ordinary service interface (`start-os`/`admin`). Dev builds that already ran
/// v0_3_6_alpha_0 have the legacy form persisted in `private.torMigration`.
fn normalize_legacy_service_identity(entry: &mut Value) {
    if entry.get("packageId").and_then(|p| p.as_str()) == Some("STARTOS") {
        entry["packageId"] = json!("start-os");
        entry["hostId"] = json!("admin");
    }
}

fn collect_ports_from_host(host: Option<&Value>, ports: &mut Value) {
    let Some(bindings) = host
        .and_then(|h| h.get("bindings"))
        .and_then(|b| b.as_object())
    else {
        return;
    };
    for (_, binding) in bindings.iter() {
        if let Some(net) = binding.get("net") {
            if let Some(port) = net.get("assignedPort").and_then(|p| p.as_u64()) {
                if let Some(obj) = ports.as_object_mut() {
                    obj.insert(port.to_string().into(), Value::from(false));
                }
            }
            if let Some(port) = net.get("assignedSslPort").and_then(|p| p.as_u64()) {
                if let Some(obj) = ports.as_object_mut() {
                    obj.insert(port.to_string().into(), Value::from(true));
                }
            }
        }
    }
}

fn migrate_available_ports(db: &mut Value) {
    let mut new_ports: Value = serde_json::json!({}).into();

    // Collect from server host
    let server_host = db
        .get("public")
        .and_then(|p| p.get("serverInfo"))
        .and_then(|s| s.get("network"))
        .and_then(|n| n.get("host"))
        .cloned();
    collect_ports_from_host(server_host.as_ref(), &mut new_ports);

    // Collect from all package hosts
    if let Some(packages) = db
        .get("public")
        .and_then(|p| p.get("packageData"))
        .and_then(|p| p.as_object())
    {
        for (_, package) in packages.iter() {
            if let Some(hosts) = package.get("hosts").and_then(|h| h.as_object()) {
                for (_, host) in hosts.iter() {
                    collect_ports_from_host(Some(host), &mut new_ports);
                }
            }
        }
    }

    // Replace private.availablePorts
    if let Some(private) = db.get_mut("private").and_then(|p| p.as_object_mut()) {
        private.insert("availablePorts".into(), new_ports);
    }
}

fn migrate_smtp(db: &mut Value) {
    if let Some(smtp) = db
        .get_mut("public")
        .and_then(|p| p.get_mut("serverInfo"))
        .and_then(|s| s.get_mut("smtp"))
        .and_then(|s| s.as_object_mut())
    {
        if let Some(server) = smtp.remove("server") {
            smtp.insert("host".into(), server);
        }
        if let Some(login) = smtp.remove("login") {
            smtp.insert("username".into(), login);
        }
        if !smtp.contains_key("security") {
            smtp.insert("security".into(), json!("starttls"));
        }
    }
}

fn denormalize_hostname(s: &str) -> String {
    let mut cap = true;
    s.chars()
        .map(|c| {
            if c == '-' {
                cap = true;
                ' '
            } else if cap {
                cap = false;
                c.to_ascii_uppercase()
            } else {
                c
            }
        })
        .collect()
}

fn migrate_host(host: Option<&mut Value>) {
    let Some(host) = host.and_then(|h| h.as_object_mut()) else {
        return;
    };

    // Remove hostnameInfo from host
    host.remove("hostnameInfo");

    // Migrate privateDomains from array to object (BTreeSet -> BTreeMap<_, BTreeSet<GatewayId>>)
    if let Some(private_domains) = host
        .get("privateDomains")
        .and_then(|v| v.as_array())
        .cloned()
    {
        let mut new_pd: Value = serde_json::json!({}).into();
        for domain in private_domains {
            if let Some(d) = domain.as_str() {
                if let Some(obj) = new_pd.as_object_mut() {
                    obj.insert(d.into(), serde_json::json!([]).into());
                }
            }
        }
        host.insert("privateDomains".into(), new_pd);
    }

    // For each binding: add "addresses" field, remove gateway-level fields from "net"
    if let Some(bindings) = host.get_mut("bindings").and_then(|b| b.as_object_mut()) {
        for (_, binding) in bindings.iter_mut() {
            if let Some(binding_obj) = binding.as_object_mut() {
                // Add addresses if not present
                if !binding_obj.contains_key("addresses") {
                    binding_obj.insert(
                        "addresses".into(),
                        serde_json::json!({
                            "enabled": [],
                            "disabled": [],
                            "available": []
                        })
                        .into(),
                    );
                }

                // Remove gateway-level privateDisabled/publicEnabled from net
                if let Some(net) = binding_obj.get_mut("net").and_then(|n| n.as_object_mut()) {
                    net.remove("privateDisabled");
                    net.remove("publicEnabled");
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn normalizes_legacy_startos_sentinel() {
        let mut legacy = json!({
            "hostname": "abcdef",
            "packageId": "STARTOS",
            "hostId": "STARTOS",
            "key": "a2V5",
        });
        normalize_legacy_service_identity(&mut legacy);
        assert_eq!(legacy["packageId"].as_str(), Some("start-os"));
        assert_eq!(legacy["hostId"].as_str(), Some("admin"));
        assert_eq!(legacy["hostname"].as_str(), Some("abcdef"));
        assert_eq!(legacy["key"].as_str(), Some("a2V5"));

        let mut package_entry = json!({
            "hostname": "abcdef",
            "packageId": "bitcoind",
            "hostId": "main",
            "key": "a2V5",
        });
        normalize_legacy_service_identity(&mut package_entry);
        assert_eq!(package_entry["packageId"].as_str(), Some("bitcoind"));
        assert_eq!(package_entry["hostId"].as_str(), Some("main"));
    }
}
