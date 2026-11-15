use std::collections::BTreeSet;
use std::fmt::{self, Display};

use serde::{Deserialize, Serialize};
use visit_rs::{VisitFields, VisitVariants};

/// Non-empty selected transports, serialized as a sorted, deduplicated array.
/// A protocol string is accepted as a singleton. Omission selects TCP and UDP.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, VisitFields)]
#[serde(try_from = "TransportWire", into = "BTreeSet<TransportProtocol>")]
#[visit(type_attributes(serde::try_from, serde::into))]
pub struct Transport(BTreeSet<TransportProtocol>);

#[derive(Deserialize, VisitVariants)]
#[serde(untagged)]
enum TransportWire {
    Single(TransportProtocol),
    Set(BTreeSet<TransportProtocol>),
}

rpc_toolkit::reflect_ts!(TransportWire);
rpc_toolkit::reflect_ts!(Transport);
rpc_toolkit::ts_export!(Transport, namespaces = [""]);

impl TryFrom<TransportWire> for Transport {
    type Error = &'static str;

    fn try_from(value: TransportWire) -> Result<Self, Self::Error> {
        let protocols = match value {
            TransportWire::Single(protocol) => BTreeSet::from([protocol]),
            TransportWire::Set(protocols) => protocols,
        };
        if protocols.is_empty() {
            return Err("at least one transport is required");
        }
        Ok(Self(protocols))
    }
}

impl From<Transport> for BTreeSet<TransportProtocol> {
    fn from(transport: Transport) -> Self {
        transport.0
    }
}

impl Default for Transport {
    fn default() -> Self {
        Self(BTreeSet::from([
            TransportProtocol::Tcp,
            TransportProtocol::Udp,
        ]))
    }
}

impl Transport {
    pub fn tcp() -> Self {
        TransportProtocol::Tcp.into()
    }

    pub fn udp() -> Self {
        TransportProtocol::Udp.into()
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        !self.0.is_disjoint(&other.0)
    }

    pub fn protocols(&self) -> &BTreeSet<TransportProtocol> {
        &self.0
    }
}

/// Comma-delimited protocol list for forwarding scripts, not the JSON representation.
impl Display for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, protocol) in self.0.iter().enumerate() {
            if i != 0 {
                f.write_str(",")?;
            }
            f.write_str(protocol.as_str())?;
        }
        Ok(())
    }
}

#[derive(
    Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, VisitVariants,
)]
#[serde(rename_all = "camelCase")]
pub enum TransportProtocol {
    Tcp,
    Udp,
}

rpc_toolkit::reflect_ts!(TransportProtocol);

impl TransportProtocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
        }
    }

    /// UPnP IGD protocol spelling.
    pub fn igd_name(self) -> &'static str {
        match self {
            Self::Tcp => "TCP",
            Self::Udp => "UDP",
        }
    }
}

impl From<TransportProtocol> for Transport {
    fn from(protocol: TransportProtocol) -> Self {
        Self(BTreeSet::from([protocol]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_sets_normalize_scalars_duplicates_and_order() {
        for (input, expected) in [
            (serde_json::json!("tcp"), serde_json::json!(["tcp"])),
            (serde_json::json!("udp"), serde_json::json!(["udp"])),
            (serde_json::json!(["tcp"]), serde_json::json!(["tcp"])),
            (serde_json::json!(["udp"]), serde_json::json!(["udp"])),
            (
                serde_json::json!(["udp", "tcp", "udp"]),
                serde_json::json!(["tcp", "udp"]),
            ),
        ] {
            let transport: Transport = serde_json::from_value(input).unwrap();
            assert_eq!(serde_json::to_value(transport).unwrap(), expected);
        }
        assert_eq!(
            serde_json::to_value(Transport::default()).unwrap(),
            serde_json::json!(["tcp", "udp"])
        );
    }

    #[test]
    fn empty_and_unknown_protocol_sets_are_rejected() {
        for input in [
            serde_json::json!([]),
            serde_json::json!("both"),
            serde_json::json!(["sctp"]),
            serde_json::Value::Null,
        ] {
            assert!(serde_json::from_value::<Transport>(input).is_err());
        }
    }
}
