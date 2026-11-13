use std::collections::BTreeMap;
use std::net::SocketAddrV4;

use super::TunnelMigration;
use crate::net::port_map::Protocol;
use crate::prelude::*;
use crate::tunnel::db::PortForward;

/// Normalizes legacy storage before typed access, preserving TCP-only ownership.
pub struct SniTransports;
impl TunnelMigration for SniTransports {
    fn action(&self, db: &mut Value) -> Result<(), Error> {
        let mut forwards: BTreeMap<SocketAddrV4, Value> = from_value(db["portForwards"].clone())?;
        for entry in forwards.values_mut() {
            if entry.get("kind").and_then(Value::as_str) != Some("sni") {
                continue;
            }
            let mut forward: PortForward = if entry.get("tcp").is_some() {
                from_value(entry.clone())?
            } else {
                let mut forward = PortForward::empty_sni();
                *forward.bindings_mut(Protocol::Tcp).unwrap() = from_value(entry.clone())?;
                forward
            };
            for protocol in Protocol::ALL {
                let bindings = forward.bindings_mut(protocol).unwrap();
                for (hostname, route) in std::mem::take(&mut bindings.routes) {
                    let hostname = hostname.to_ascii_lowercase();
                    match bindings.routes.get(&hostname) {
                        Some(existing)
                            if !route.enabled
                                || (existing.enabled
                                    && existing.target.ip() != route.target.ip()) => {}
                        _ => {
                            bindings.routes.insert(hostname, route);
                        }
                    }
                }
            }
            *entry = to_value(&forward)?;
        }
        db["portForwards"] = to_value(&forwards)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_legacy_tcp_intent_without_copying_ownership_or_changing_dnat() {
        let mut db = imbl_value::json!({
            "portForwards": {
                "203.0.113.1:443": {
                    "kind": "sni",
                    "routes": { "tls.example.com": { "target": "10.59.0.2:443", "label": "TLS", "enabled": false, "auto": true } },
                    "fallback": { "target": "10.59.0.3:443", "label": null, "enabled": false, "auto": true }
                },
                "203.0.113.1:8443": { "kind": "dnat", "target": "10.59.0.4:8443", "label": "plain", "count": 1 }
            }
        });
        let routes = db["portForwards"]["203.0.113.1:443"]["routes"].clone();
        let fallback = db["portForwards"]["203.0.113.1:443"]["fallback"].clone();
        let dnat = db["portForwards"]["203.0.113.1:8443"].clone();
        SniTransports.action(&mut db).unwrap();
        let entry = &db["portForwards"]["203.0.113.1:443"];
        assert_eq!(entry["tcp"]["routes"], routes);
        assert_eq!(entry["tcp"]["fallback"], fallback);
        assert!(entry.get("routes").is_none());
        assert!(entry.get("fallback").is_none());
        for bucket in ["udp", "both"] {
            assert_eq!(
                entry[bucket],
                imbl_value::json!({ "routes": {}, "fallback": null })
            );
        }
        assert_eq!(db["portForwards"]["203.0.113.1:8443"], dnat);
        let once = db.clone();
        SniTransports.action(&mut db).unwrap();
        assert_eq!(db, once);
    }

    #[test]
    fn lowercases_legacy_route_keys_preferring_the_enabled_route() {
        let mut db = imbl_value::json!({
            "portForwards": {
                "203.0.113.1:443": {
                    "kind": "sni",
                    "routes": {
                        "Cloud.Example.com": { "target": "10.59.0.2:443", "label": null, "enabled": false, "auto": false },
                        "cloud.example.com": { "target": "10.59.0.3:443", "label": null, "enabled": true, "auto": true },
                        "Mail.Example.com": { "target": "10.59.0.4:443", "label": "mail", "enabled": true, "auto": false },
                        "Api.Example.com": { "target": "10.59.0.5:443", "label": "old port", "enabled": true, "auto": false },
                        "api.example.com": { "target": "10.59.0.5:8443", "label": "active port", "enabled": true, "auto": false },
                        "Blog.Example.com": { "target": "10.59.0.6:443", "label": "active device", "enabled": true, "auto": false },
                        "blog.example.com": { "target": "10.59.0.7:443", "label": "conflicting device", "enabled": true, "auto": true }
                    }
                }
            }
        });
        SniTransports.action(&mut db).unwrap();
        let entry = &db["portForwards"]["203.0.113.1:443"];
        let routes = &entry["tcp"]["routes"];
        assert_eq!(routes.as_object().unwrap().len(), 4);
        for (hostname, target) in [
            ("cloud.example.com", "10.59.0.3:443"),
            ("mail.example.com", "10.59.0.4:443"),
            ("api.example.com", "10.59.0.5:8443"),
            ("blog.example.com", "10.59.0.6:443"),
        ] {
            assert_eq!(routes[hostname]["target"], Value::from(target));
        }
        assert_eq!(entry["tcp"]["fallback"], Value::Null);
        let once = db.clone();
        SniTransports.action(&mut db).unwrap();
        assert_eq!(db, once);
    }

    #[test]
    fn normalized_buckets_preserve_independent_metadata() {
        let mut entry = PortForward::empty_sni();
        for (protocol, hostname, target, enabled, auto) in [
            (
                Protocol::Tcp,
                "tls.example.com",
                "10.59.0.2:443",
                false,
                true,
            ),
            (
                Protocol::Udp,
                "quic.example.com",
                "10.59.0.3:8443",
                true,
                false,
            ),
            (
                Protocol::TcpUdp,
                "both.example.com",
                "10.59.0.4:443",
                false,
                false,
            ),
        ] {
            entry.bindings_mut(protocol).unwrap().routes.insert(
                hostname.into(),
                crate::tunnel::db::SniRoute {
                    target: target.parse().unwrap(),
                    label: Some(hostname.into()),
                    enabled,
                    auto,
                },
            );
        }
        let mut db = imbl_value::json!({ "portForwards": {} });
        db["portForwards"]["203.0.113.1:443"] = to_value(&entry).unwrap();
        let before = db.clone();
        SniTransports.action(&mut db).unwrap();
        assert_eq!(db, before);
    }

    #[test]
    fn startup_migrates_legacy_storage_before_typed_database_access() {
        use patch_db::ModelExt;

        use crate::tunnel::db::TunnelDatabase;
        use crate::tunnel::migrations::{MIGRATIONS, run_migrations};

        let mut db = Model::new(&TunnelDatabase::default()).unwrap();
        ModelExt::as_value_mut(&mut db)["portForwards"] = imbl_value::json!({
            "203.0.113.1:443": {
                "kind": "sni",
                "routes": { "TLS.Example.com": { "target": "10.59.0.2:443", "label": "PCP", "enabled": false } },
                "fallback": { "target": "10.59.0.3:443", "label": null, "enabled": false, "auto": false }
            }
        });
        run_migrations(&mut db).unwrap();
        let typed = db.de().unwrap();
        assert_eq!(typed.migrations.len(), MIGRATIONS.len());
        let entry = &typed.port_forwards.0[&"203.0.113.1:443".parse().unwrap()];
        let route = &entry.bindings(Protocol::Tcp).unwrap().routes["tls.example.com"];
        assert_eq!(route.target, "10.59.0.2:443".parse().unwrap());
        assert_eq!(route.label.as_deref(), Some("PCP"));
        assert!(!route.enabled);
        assert!(route.auto);
        let fallback = entry
            .bindings(Protocol::Tcp)
            .unwrap()
            .fallback
            .as_ref()
            .unwrap();
        assert!(!fallback.auto);
        assert!(!fallback.enabled);
        assert!(fallback.label.is_none());
        assert!(entry.bindings(Protocol::Udp).unwrap().is_empty());
        assert!(entry.bindings(Protocol::TcpUdp).unwrap().is_empty());
        let once = ModelExt::as_value_mut(&mut db).clone();
        run_migrations(&mut db).unwrap();
        assert_eq!(ModelExt::as_value_mut(&mut db).clone(), once);
    }
}
