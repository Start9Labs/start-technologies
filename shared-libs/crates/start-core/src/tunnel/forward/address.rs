use std::fmt::{self, Display};
use std::net::{SocketAddrV4, SocketAddrV6};
use std::ops::Deref;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use visit_rs::VisitFields;

use crate::net::transport::Transport;

macro_rules! transport_address {
    ($name:ident, $socket:ty) => {
        /// Transport-qualified keys use `tcp@socket` or `udp@socket`; socket-only keys retain both.
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, VisitFields,
        )]
        #[serde(try_from = "String", into = "String")]
        #[visit(type_attributes(serde::try_from, serde::into))]
        pub struct $name {
            pub address: $socket,
            pub transport: Transport,
        }

        rpc_toolkit::reflect_ts!($name);

        impl $name {
            pub fn new(address: $socket, transport: Transport) -> Self {
                Self { address, transport }
            }
        }
        impl From<$socket> for $name {
            fn from(address: $socket) -> Self {
                Self::new(address, Transport::Both)
            }
        }
        impl Deref for $name {
            type Target = $socket;
            fn deref(&self) -> &$socket {
                &self.address
            }
        }
        impl Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self.transport {
                    Transport::Both => self.address.fmt(f),
                    transport => write!(f, "{}@{}", transport.as_str(), self.address),
                }
            }
        }
        impl From<$name> for String {
            fn from(key: $name) -> Self {
                key.to_string()
            }
        }
        impl TryFrom<String> for $name {
            type Error = String;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                value.parse()
            }
        }
        impl FromStr for $name {
            type Err = String;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let (transport, address) = match value.split_once('@') {
                    Some(("tcp", address)) => (Transport::Tcp, address),
                    Some(("udp", address)) => (Transport::Udp, address),
                    Some(_) => return Err("invalid forward transport".into()),
                    None => (Transport::Both, value),
                };
                Ok(Self::new(
                    address.parse::<$socket>().map_err(|e| e.to_string())?,
                    transport,
                ))
            }
        }
    };
}

transport_address!(ForwardAddress, SocketAddrV4);
transport_address!(PinholeAddress, SocketAddrV6);
