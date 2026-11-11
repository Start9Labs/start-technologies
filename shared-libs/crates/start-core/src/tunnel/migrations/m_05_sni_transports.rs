use super::TunnelMigration;
use crate::prelude::*;
use crate::tunnel::db::{PortForward, PortForwards};

/// Legacy hostname routes and fallbacks retain TCP ownership.
/// Canonicalizes legacy keys while retaining the demux's enabled owner.
pub struct SniTransports;
impl TunnelMigration for SniTransports {
    fn action(&self, db: &mut Value) -> Result<(), Error> {
        let mut forwards: PortForwards = from_value(db["portForwards"].clone())?;
        for forward in forwards.0.values_mut() {
            if let PortForward::Sni { tcp, udp } = forward {
                for bindings in [tcp, udp] {
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
            }
        }
        db["portForwards"] = to_value(&forwards)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materializes_empty_udp_without_copying_tcp_ownership() {
        let mut db = imbl_value::json!({
            "portForwards": {
                "203.0.113.1:443": {
                    "kind": "sni",
                    "routes": { "tls.example.com": { "target": "10.59.0.2:443", "label": "TLS", "enabled": false, "auto": true } },
                    "fallback": { "target": "10.59.0.3:443", "label": null }
                },
                "203.0.113.1:8443": { "kind": "dnat", "target": "10.59.0.4:8443", "label": "plain", "count": 1 }
            }
        });
        SniTransports.action(&mut db).unwrap();
        assert_eq!(
            db["portForwards"]["203.0.113.1:443"]["udp"],
            imbl_value::json!({ "routes": {}, "fallback": null })
        );
        assert_eq!(
            db["portForwards"]["203.0.113.1:443"]["routes"]["tls.example.com"]["enabled"],
            Value::Bool(false)
        );
        assert_eq!(
            db["portForwards"]["203.0.113.1:443"]["fallback"]["target"],
            Value::from("10.59.0.3:443")
        );
        assert_eq!(
            db["portForwards"]["203.0.113.1:8443"]["kind"],
            Value::from("dnat")
        );
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
                    },
                    "fallback": null
                }
            }
        });
        SniTransports.action(&mut db).unwrap();
        let routes = &db["portForwards"]["203.0.113.1:443"]["routes"];
        assert_eq!(routes.as_object().unwrap().len(), 4);
        assert_eq!(
            routes["cloud.example.com"]["target"],
            Value::from("10.59.0.3:443")
        );
        assert_eq!(
            routes["mail.example.com"]["target"],
            Value::from("10.59.0.4:443")
        );
        assert_eq!(
            routes["api.example.com"]["target"],
            Value::from("10.59.0.5:8443")
        );
        assert_eq!(
            routes["blog.example.com"]["target"],
            Value::from("10.59.0.6:443")
        );
        let once = db.clone();
        SniTransports.action(&mut db).unwrap();
        assert_eq!(db, once);
    }
}
