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
