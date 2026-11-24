use futures_util::stream::{FuturesUnordered, StreamExt};
use hickory_net::proto::op::Message;
use rand::seq::SliceRandom;
use tracing::debug;

use crate::config::Method;
use crate::upstream::Upstream;

/// The first final answer that `method` gets, else the last answer of any kind.
pub async fn resolve(upstreams: &[Upstream], method: Method, query: &Message) -> Option<Vec<u8>> {
    match method {
        Method::InOrder => in_order(upstreams.iter(), query).await,
        Method::Random => {
            let mut shuffled: Vec<_> = upstreams.iter().collect();
            shuffled.shuffle(&mut rand::rng());
            in_order(shuffled.into_iter(), query).await
        }
        Method::Fastest => {
            let mut pending: FuturesUnordered<_> =
                upstreams.iter().map(|u| attempt(u, query)).collect();
            let mut fallback = None;
            while let Some(answer) = pending.next().await {
                match answer {
                    Some(answer) if is_final(&answer) => return Some(answer),
                    Some(answer) => fallback = Some(answer),
                    None => {}
                }
            }
            fallback
        }
    }
}

async fn in_order<'a>(
    upstreams: impl Iterator<Item = &'a Upstream>,
    query: &Message,
) -> Option<Vec<u8>> {
    let mut fallback = None;
    for upstream in upstreams {
        match attempt(upstream, query).await {
            Some(answer) if is_final(&answer) => return Some(answer),
            Some(answer) => fallback = Some(answer),
            None => {}
        }
    }
    fallback
}

async fn attempt(upstream: &Upstream, query: &Message) -> Option<Vec<u8>> {
    let addr = upstream.server.socket_addr();
    match tokio::time::timeout(upstream.server.timeout(), upstream.exchange(query)).await {
        Ok(Ok(answer)) => Some(answer),
        Ok(Err(e)) => {
            debug!(%addr, "upstream failed: {e}");
            None
        }
        Err(_) => {
            debug!(%addr, "upstream timed out");
            None
        }
    }
}

/// NOERROR and NXDOMAIN end the search; every other code tries the next server.
fn is_final(answer: &[u8]) -> bool {
    matches!(answer.get(3).map(|flags| flags & 0x0F), Some(0 | 3))
}
