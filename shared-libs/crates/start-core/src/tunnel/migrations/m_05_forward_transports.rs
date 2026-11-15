use super::TunnelMigration;
use crate::prelude::*;
use crate::tunnel::db::{Pinholes6, PortForwards};

pub struct ForwardTransports;

impl TunnelMigration for ForwardTransports {
    fn action(&self, db: &mut Value) -> Result<(), Error> {
        if let Some(value) = db.get_mut("portForwards") {
            let mut forwards: PortForwards = imbl_value::from_value(value.clone())?;
            forwards.split_auto_transports();
            *value = imbl_value::to_value(&forwards)?;
        }
        if let Some(value) = db.get_mut("pinholes6") {
            let mut pinholes: Pinholes6 = imbl_value::from_value(value.clone())?;
            pinholes.split_auto_transports();
            *value = imbl_value::to_value(&pinholes)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use imbl_value::json;

    use super::*;

    #[test]
    fn legacy_auto_transports_split_once_without_changing_operator_choices() {
        let mut db = json!({
            "portForwards": {
                "192.0.2.1:5000": {"kind":"dnat", "target":"10.59.0.2:6000", "enabled":false, "count":2, "label":"media", "auto":true},
                "192.0.2.1:7000": {"kind":"dnat", "target":"10.59.0.2:7000", "enabled":true, "count":1, "label":null, "auto":false}
            },
            "pinholes6": {"[2001:db8::2]:5000": {"enabled":false,"count":2,"internalPort":null,"label":"media","auto":true}}
        });
        let legacy_forward = db["portForwards"]["192.0.2.1:5000"].clone();
        let legacy_pinhole = db["pinholes6"]["[2001:db8::2]:5000"].clone();
        ForwardTransports.action(&mut db).unwrap();
        for protocol in ["tcp", "udp"] {
            assert_eq!(
                db["portForwards"][format!("{protocol}@192.0.2.1:5000")],
                legacy_forward
            );
            assert_eq!(
                db["pinholes6"][format!("{protocol}@[2001:db8::2]:5000")],
                legacy_pinhole
            );
        }
        assert!(db["portForwards"].get("192.0.2.1:7000").is_some());
        assert_eq!(db["portForwards"].as_object().unwrap().len(), 3);
        let once = db.clone();
        ForwardTransports.action(&mut db).unwrap();
        assert_eq!(db, once);
    }
}
