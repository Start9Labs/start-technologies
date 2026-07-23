use std::net::SocketAddrV4;

use imbl_value::{InternedString, json};

use super::TunnelMigration;
use crate::prelude::*;

/// Reclaim port 80 for the HTTP→HTTPS redirect on every public IPv4: clear the
/// per-IP redirect opt-outs and drop every forward that occupies port 80, so
/// port 80 is the redirect rather than a published port. A missing
/// `httpRedirects` already means "on everywhere", so it is left alone.
pub struct ReclaimPort80Redirect;
impl TunnelMigration for ReclaimPort80Redirect {
    fn action(&self, db: &mut Value) -> Result<(), Error> {
        let Some(root) = db.as_object_mut() else {
            return Ok(());
        };
        if let Some(redirects) = root
            .get_mut("httpRedirects")
            .and_then(|v| v.as_object_mut())
        {
            redirects.insert(InternedString::intern("disabled"), json!([]));
        }
        if let Some(forwards) = root.get_mut("portForwards").and_then(|v| v.as_object_mut()) {
            forwards.retain(|src, forward| !occupies_port_80(src, forward));
        }
        Ok(())
    }
}

/// Whether a forward's external port span covers port 80. A DNAT spans `count`
/// contiguous ports from its key port (default 1); an SNI forward spans 1. This
/// mirrors `PortForwards::occupied`, so the migration frees port 80 wherever the
/// runtime would otherwise see it taken. Unparseable keys are treated as not
/// occupying it (left untouched).
fn occupies_port_80(src: &str, forward: &Value) -> bool {
    let Ok(addr) = src.parse::<SocketAddrV4>() else {
        return false;
    };
    let span = forward
        .get("count")
        .and_then(|c| c.as_u64())
        .and_then(|c| u16::try_from(c).ok())
        .unwrap_or(1)
        .max(1);
    let hi = addr.port().saturating_add(span - 1);
    addr.port() <= 80 && 80 <= hi
}

#[cfg(test)]
mod test {
    use imbl_value::json;

    use super::{ReclaimPort80Redirect, TunnelMigration};

    #[test]
    fn clears_opt_outs_and_drops_port_80_forwards() {
        let mut db = json!({
            "httpRedirects": { "disabled": ["1.2.3.4", "5.6.7.8"] },
            "portForwards": {
                "1.2.3.4:80": { "kind": "dnat", "target": "10.59.0.2:80", "label": null, "enabled": true, "count": 1, "auto": false },
                "1.2.3.4:443": { "kind": "dnat", "target": "10.59.0.2:443", "label": null, "enabled": true, "count": 1, "auto": false },
                "5.6.7.8:80": { "kind": "sni", "routes": {}, "fallback": null },
                "9.9.9.9:78": { "kind": "dnat", "target": "10.59.0.2:78", "label": null, "enabled": true, "count": 5, "auto": false },
                "6.6.6.6:78": { "kind": "dnat", "target": "10.59.0.2:78", "label": null, "enabled": true, "count": 2, "auto": false },
                "7.7.7.7:81": { "kind": "dnat", "target": "10.59.0.2:81", "label": null, "enabled": true, "count": 3, "auto": false },
                "80.0.0.1:443": { "kind": "dnat", "target": "10.59.0.2:443", "label": null, "enabled": true, "count": 1, "auto": false }
            }
        });

        ReclaimPort80Redirect.action(&mut db).unwrap();

        assert_eq!(db["httpRedirects"]["disabled"], json!([]));

        let forwards = db["portForwards"].as_object().unwrap();
        // Dropped: a single :80 dnat, an :80 sni, and a range that spans 80.
        assert!(!forwards.contains_key("1.2.3.4:80"));
        assert!(!forwards.contains_key("5.6.7.8:80"));
        assert!(!forwards.contains_key("9.9.9.9:78"));
        // Kept: other ports, and ranges near but not covering 80.
        assert!(forwards.contains_key("1.2.3.4:443"));
        assert!(forwards.contains_key("6.6.6.6:78")); // 78..=79
        assert!(forwards.contains_key("7.7.7.7:81")); // 81..=83
        assert!(forwards.contains_key("80.0.0.1:443")); // IP has "80", port 443
    }

    #[test]
    fn no_op_when_http_redirects_absent() {
        let mut db = json!({
            "portForwards": {
                "1.2.3.4:80": { "kind": "dnat", "target": "10.59.0.2:80", "label": null, "enabled": true, "count": 1, "auto": false }
            }
        });

        ReclaimPort80Redirect.action(&mut db).unwrap();

        assert!(db.as_object().unwrap().get("httpRedirects").is_none());
        assert!(
            !db["portForwards"]
                .as_object()
                .unwrap()
                .contains_key("1.2.3.4:80")
        );
    }
}
