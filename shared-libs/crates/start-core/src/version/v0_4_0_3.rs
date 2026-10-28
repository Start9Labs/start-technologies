use exver::VersionRange;

use super::v0_3_5::V0_3_0_COMPAT;
use super::{VersionT, v0_4_0_2};
use crate::prelude::*;

lazy_static::lazy_static! {
    static ref V0_4_0_3: exver::Version = exver::Version::new([0, 4, 0, 3], []);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Version;

impl VersionT for Version {
    type Previous = v0_4_0_2::Version;
    type PreUpRes = ();

    async fn pre_up(self) -> Result<Self::PreUpRes, Error> {
        Ok(())
    }
    fn semver(self) -> exver::Version {
        V0_4_0_3.clone()
    }
    fn compat(self) -> &'static VersionRange {
        &V0_3_0_COMPAT
    }
    fn migration_revision(self) -> usize {
        3
    }
    fn up(self, db: &mut Value, _: Self::PreUpRes) -> Result<Value, Error> {
        migrate_gateway_dns(db)?;
        default_outbound_dns(db);
        drop_package_icons(db);
        seed_update_signers(db)?;
        Ok(Value::Null)
    }
    fn down(self, _db: &mut Value) -> Result<(), Error> {
        Ok(())
    }
}

fn seed_update_signers(db: &mut Value) -> Result<(), Error> {
    if let Some(server) = db
        .get_mut("public")
        .and_then(|public| public.get_mut("serverInfo"))
        .and_then(|server| server.as_object_mut())
    {
        if !server.contains_key("trustedOsUpdateSigners") {
            server.insert(
                "trustedOsUpdateSigners".into(),
                to_value(&crate::update::signers::initial_signers())?,
            );
        }
    }
    Ok(())
}

fn migrate_gateway_dns(db: &mut Value) -> Result<(), Error> {
    use crate::db::model::public::{DnsSettings, IpInfo};

    let Some(network) = db
        .get_mut("public")
        .and_then(|p| p.get_mut("serverInfo"))
        .and_then(|s| s.get_mut("network"))
        .and_then(|n| n.as_object_mut())
    else {
        return Ok(());
    };
    let legacy = network.remove("dns");
    let overrides = legacy
        .as_ref()
        .and_then(|d| d.get("staticServers"))
        .filter(|s| !s.is_null())
        .cloned();
    if let Some(gateways) = network.get_mut("gateways").and_then(|g| g.as_object_mut()) {
        for (_, gateway) in gateways.iter_mut() {
            let Some(gateway) = gateway.as_object_mut() else {
                continue;
            };
            if gateway.contains_key("dns") {
                continue;
            }
            let mut dns = DnsSettings::default();
            if let Some(overrides) = overrides.as_ref() {
                let mut servers: std::collections::VecDeque<std::net::SocketAddr> =
                    from_value(overrides.clone())?;
                // Empty overrides request public DNS.
                if servers.is_empty() {
                    servers = dns.servers.clone();
                }
                dns.static_servers = Some(servers);
            }
            let ip_info: Option<IpInfo> = gateway
                .get("ipInfo")
                .cloned()
                .map(from_value)
                .transpose()?
                .flatten();
            dns.refresh(ip_info.as_ref());
            gateway.insert("dns".into(), to_value(&dns)?);
        }
    }
    Ok(())
}

fn default_outbound_dns(db: &mut Value) {
    let Some(public) = db.get_mut("public") else {
        return;
    };
    if let Some(network) = public
        .get_mut("serverInfo")
        .and_then(|s| s.get_mut("network"))
        .and_then(|n| n.as_object_mut())
    {
        network.entry("outboundDns".into()).or_insert(Value::Null);
    }
    if let Some(packages) = public
        .get_mut("packageData")
        .and_then(|p| p.as_object_mut())
    {
        for (_, package) in packages.iter_mut() {
            if let Some(package) = package.as_object_mut() {
                package.entry("outboundDns".into()).or_insert(Value::Null);
            }
        }
    }
}

fn drop_package_icons(db: &mut Value) {
    let Some(packages) = db
        .get_mut("public")
        .and_then(|p| p.get_mut("packageData"))
        .and_then(|p| p.as_object_mut())
    else {
        return;
    };
    for (_, package) in packages.iter_mut() {
        let Some(package) = package.as_object_mut() else {
            continue;
        };
        package.remove("icon");
        if let Some(deps) = package
            .get_mut("currentDependencies")
            .and_then(|d| d.as_object_mut())
        {
            for (_, dep) in deps.iter_mut() {
                if let Some(dep) = dep.as_object_mut() {
                    dep.remove("icon");
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use imbl_value::json;

    use super::*;

    #[test]
    fn seeds_update_signers_once_and_preserves_saved_trust() {
        let mut db = json!({ "public": { "serverInfo": {} } });
        seed_update_signers(&mut db).unwrap();
        assert_eq!(
            db["public"]["serverInfo"]["trustedOsUpdateSigners"],
            to_value(&crate::update::signers::initial_signers()).unwrap()
        );
        let once = db.clone();
        seed_update_signers(&mut db).unwrap();
        assert_eq!(db, once);

        let mut saved = crate::update::signers::initial_signers();
        saved.pop_first();
        saved.values_mut().next().unwrap().label = "Owner-managed".to_owned();
        db["public"]["serverInfo"]["trustedOsUpdateSigners"] = to_value(&saved).unwrap();
        let custom = db.clone();
        seed_update_signers(&mut db).unwrap();
        assert_eq!(db, custom);
        db["public"]["serverInfo"]["trustedOsUpdateSigners"] = json!({});
        seed_update_signers(&mut db).unwrap();
        assert_eq!(
            db["public"]["serverInfo"]["trustedOsUpdateSigners"],
            json!({})
        );
        assert_eq!(Version.migration_revision(), 3);
    }

    #[test]
    fn migrates_global_dns_once_and_preserves_gateway_overrides() {
        let mut db = json!({ "public": { "serverInfo": { "network": {
            "dns": { "dhcpServers": ["192.0.2.1:53"], "staticServers": ["192.0.2.53:5353"] },
            "gateways": {
                "eth0": { "ipInfo": null },
                "wg0": { "ipInfo": null, "dns": { "staticServers": null, "servers": ["10.64.0.1:53"], "source": "gateway" } }
            }
        } } } });
        migrate_gateway_dns(&mut db).unwrap();
        let network = &db["public"]["serverInfo"]["network"];
        assert!(network.get("dns").is_none());
        assert_eq!(
            network["gateways"]["eth0"]["dns"],
            json!({ "staticServers": ["192.0.2.53:5353"], "servers": ["192.0.2.53:5353"], "source": "static" })
        );
        assert_eq!(
            network["gateways"]["wg0"]["dns"]["staticServers"],
            Value::Null
        );
        let once = db.clone();
        migrate_gateway_dns(&mut db).unwrap();
        assert_eq!(db, once);
    }

    #[test]
    fn empty_legacy_dns_preserves_public_dns_intent() {
        let mut db = json!({ "public": { "serverInfo": { "network": {
            "dns": { "dhcpServers": [], "staticServers": [] },
            "gateways": { "eth0": { "ipInfo": null } }
        } } } });
        migrate_gateway_dns(&mut db).unwrap();
        let dns = &db["public"]["serverInfo"]["network"]["gateways"]["eth0"]["dns"];
        assert_eq!(
            dns["staticServers"],
            json!(["1.1.1.1:53", "[2606:4700:4700::1111]:53"])
        );
        assert_eq!(dns["source"], json!("static"));
    }

    #[test]
    fn requester_dns_defaults_once_without_overwriting_saved_intent() {
        let mut db = json!({ "public": {
            "serverInfo": { "network": {} },
            "packageData": { "old": {}, "saved": { "outboundDns": ["192.0.2.53:53"] } }
        } });
        default_outbound_dns(&mut db);
        assert_eq!(
            db["public"]["serverInfo"]["network"]["outboundDns"],
            Value::Null
        );
        assert_eq!(
            db["public"]["packageData"]["old"]["outboundDns"],
            Value::Null
        );
        assert_eq!(
            db["public"]["packageData"]["saved"]["outboundDns"],
            json!(["192.0.2.53:53"])
        );
        let once = db.clone();
        default_outbound_dns(&mut db);
        assert_eq!(db, once);
    }

    #[test]
    fn up_drops_package_and_dependency_icons() {
        let mut db = json!({
            "public": { "packageData": { "pkg": {
                "icon": "data:image/png;base64,aWNvbg==",
                "currentDependencies": { "dep": {
                    "title": "Dep",
                    "icon": "data:image/png;base64,aWNvbg==",
                    "kind": "exists",
                    "versionRange": "*",
                } },
            } } }
        });
        Version.up(&mut db, ()).unwrap();
        assert_eq!(
            db,
            json!({
                "public": { "packageData": { "pkg": {
                    "outboundDns": null,
                    "currentDependencies": { "dep": {
                        "title": "Dep",
                        "kind": "exists",
                        "versionRange": "*",
                    } },
                } } }
            })
        );
    }
}
