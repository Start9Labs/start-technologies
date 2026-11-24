//! A DNS forwarder for one StartWRT profile: it answers on a loopback port and forwards each
//! query unchanged to the profile's servers, over sockets bound to the profile's interface.

mod bind;
pub mod config;
mod listen;
mod methods;
mod upstream;

use hickory_net::proto::op::{Edns, Message, MessageType, OpCode, ResponseCode};

pub use crate::bind::BoundProvider;
use crate::config::{Config, Method};
pub use crate::listen::serve;
use crate::upstream::{tls_config, Error, Upstream};

pub struct Forwarder {
    upstreams: Vec<Upstream>,
    method: Method,
}

/// UDP payload size advertised in replies that the forwarder writes itself.
const EDNS_PAYLOAD: u16 = 1232;

impl Forwarder {
    pub fn new(config: &Config) -> Result<Self, Error> {
        let provider = BoundProvider::new(config.interface.as_deref());
        let tls = tls_config()?;
        Ok(Self {
            upstreams: config
                .servers
                .iter()
                .map(|server| Upstream::new(server.clone(), provider.clone(), tls.clone()))
                .collect(),
            method: config.method,
        })
    }

    /// The reply to one query, or `None` for a message that gets no reply.
    pub async fn answer(&self, query: &[u8], udp: bool) -> Option<Vec<u8>> {
        let query = match Message::from_vec(query) {
            Ok(query) => query,
            Err(_) => {
                let id = u16::from_be_bytes(query.get(..2)?.try_into().ok()?);
                return Message::error_msg(id, OpCode::Query, ResponseCode::FormErr)
                    .to_vec()
                    .ok();
            }
        };
        if query.metadata.message_type != MessageType::Query {
            return None;
        }
        let answer = match methods::resolve(&self.upstreams, self.method, &query).await {
            Some(answer) => answer,
            None => return reply(&query, ResponseCode::ServFail, false),
        };
        if udp && answer.len() > usize::from(query.max_payload()) {
            return reply(&query, ResponseCode::NoError, true);
        }
        Some(answer)
    }
}

fn reply(query: &Message, code: ResponseCode, truncated: bool) -> Option<Vec<u8>> {
    let mut reply = Message::error_msg(query.metadata.id, query.metadata.op_code, code);
    reply.metadata.recursion_desired = query.metadata.recursion_desired;
    reply.metadata.recursion_available = true;
    reply.metadata.checking_disabled = query.metadata.checking_disabled;
    reply.metadata.truncation = truncated;
    reply.queries = query.queries.clone();
    reply.edns = query.edns.as_ref().map(|query| {
        let mut edns = Edns::new();
        edns.set_max_payload(EDNS_PAYLOAD);
        edns.set_dnssec_ok(query.flags().dnssec_ok);
        edns
    });
    reply.to_vec().ok()
}

#[cfg(test)]
mod tests;
