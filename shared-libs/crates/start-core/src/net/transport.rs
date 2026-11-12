use serde::{Deserialize, Serialize};
use visit_rs::VisitVariants;

/// Transports forwarded by a binding. Omitted declarations retain TCP and UDP.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Deserialize,
    Serialize,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    VisitVariants,
)]
#[serde(rename_all = "camelCase")]
pub enum Transport {
    Tcp,
    Udp,
    #[default]
    Both,
}

rpc_toolkit::reflect_ts!(Transport);
rpc_toolkit::ts_export!(Transport, namespaces = [""]);

impl Transport {
    pub fn overlaps(self, other: Self) -> bool {
        self.protocols()
            .iter()
            .any(|p| other.protocols().contains(p))
    }

    pub fn protocols(self) -> &'static [TransportProtocol] {
        match self {
            Self::Tcp => &[TransportProtocol::Tcp],
            Self::Udp => &[TransportProtocol::Udp],
            Self::Both => &[TransportProtocol::Tcp, TransportProtocol::Udp],
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
            Self::Both => "both",
        }
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

impl From<TransportProtocol> for Transport {
    fn from(protocol: TransportProtocol) -> Self {
        match protocol {
            TransportProtocol::Tcp => Self::Tcp,
            TransportProtocol::Udp => Self::Udp,
        }
    }
}
