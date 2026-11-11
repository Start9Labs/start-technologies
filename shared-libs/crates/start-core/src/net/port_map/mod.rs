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

/// Transport identity for independently owned port mappings.
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
