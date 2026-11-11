use super::TunnelMigration;
use crate::prelude::*;
use crate::tunnel::db::PortForwards;

/// Legacy hostname routes and fallbacks retain TCP ownership.
pub struct SniTransports;
impl TunnelMigration for SniTransports {
    fn action(&self, db: &mut Value) -> Result<(), Error> {
        let forwards: PortForwards = from_value(db["portForwards"].clone())?;
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
}
