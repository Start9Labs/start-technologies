//! NAT port-mapping protocols: a client that asks an upstream gateway to open
//! ports (PCP/NAT-PMP/UPnP) and a reusable server that answers such requests
//! (see [`server`]).

mod client;
pub mod pcp;
pub mod probe;
pub mod server;
pub mod upnp;

pub(crate) use client::set_verdict;
pub use client::{PortMapController, candidate_gateways};

/// Transport of a gateway request or dataplane listener.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    visit_rs::VisitVariants,
    clap::ValueEnum,
)]
#[serde(rename_all = "lowercase")]
pub enum TransportProtocol {
    #[default]
    Tcp,
    Udp,
}

rpc_toolkit::reflect_ts!(TransportProtocol);

/// Transports owned together by one port mapping.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    visit_rs::VisitVariants,
    clap::ValueEnum,
)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    #[default]
    Tcp,
    Udp,
    #[serde(rename = "tcp+udp")]
    #[value(name = "tcp+udp")]
    TcpUdp,
}

rpc_toolkit::reflect_ts!(Protocol);

impl Protocol {
    pub const ALL: [Self; 3] = [Self::Tcp, Self::Udp, Self::TcpUdp];

    pub fn transports(self) -> &'static [TransportProtocol] {
        match self {
            Self::Tcp => &[TransportProtocol::Tcp],
            Self::Udp => &[TransportProtocol::Udp],
            Self::TcpUdp => &TransportProtocol::ALL,
        }
    }

    pub fn contains(self, transport: TransportProtocol) -> bool {
        self.transports().contains(&transport)
    }

    pub fn overlaps(self, other: Self) -> bool {
        self.transports()
            .iter()
            .any(|&transport| other.contains(transport))
    }
}

impl From<TransportProtocol> for Protocol {
    fn from(transport: TransportProtocol) -> Self {
        match transport {
            TransportProtocol::Tcp => Self::Tcp,
            TransportProtocol::Udp => Self::Udp,
        }
    }
}

impl TransportProtocol {
    pub const ALL: [Self; 2] = [Self::Tcp, Self::Udp];

    pub fn ip_number(self) -> u8 {
        match self {
            Self::Tcp => 6,
            Self::Udp => 17,
        }
    }

    pub fn from_ip_number(number: u8) -> Option<Self> {
        match number {
            6 => Some(Self::Tcp),
            17 => Some(Self::Udp),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
        }
    }

    pub fn igd(self) -> &'static str {
        match self {
            Self::Tcp => "TCP",
            Self::Udp => "UDP",
        }
    }

    pub fn from_igd(protocol: &str) -> Option<Self> {
        match protocol {
            "TCP" => Some(Self::Tcp),
            "UDP" => Some(Self::Udp),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_protocol_owns_the_selected_transports() {
        assert_eq!(Protocol::default(), Protocol::Tcp);
        assert_eq!(Protocol::Tcp.transports(), &[TransportProtocol::Tcp]);
        assert_eq!(Protocol::Udp.transports(), &[TransportProtocol::Udp]);
        assert_eq!(
            Protocol::TcpUdp.transports(),
            &[TransportProtocol::Tcp, TransportProtocol::Udp]
        );
        assert!(!Protocol::Tcp.overlaps(Protocol::Udp));
        assert!(!Protocol::Udp.overlaps(Protocol::Tcp));
        for protocol in Protocol::ALL {
            assert!(protocol.overlaps(protocol));
            assert!(protocol.overlaps(Protocol::TcpUdp));
            assert!(Protocol::TcpUdp.overlaps(protocol));
        }
    }

    #[test]
    fn mapping_protocol_preserves_existing_wire_values() {
        for (protocol, wire) in [
            (Protocol::Tcp, "tcp"),
            (Protocol::Udp, "udp"),
            (Protocol::TcpUdp, "tcp+udp"),
        ] {
            assert_eq!(serde_json::to_value(protocol).unwrap(), wire);
            assert_eq!(
                serde_json::from_value::<Protocol>(serde_json::json!(wire)).unwrap(),
                protocol
            );
        }
        assert_eq!(Protocol::from(TransportProtocol::Tcp), Protocol::Tcp);
        assert_eq!(Protocol::from(TransportProtocol::Udp), Protocol::Udp);
    }
}
