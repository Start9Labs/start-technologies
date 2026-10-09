use std::collections::BTreeMap;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Mutex as SyncMutex;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::extract::Request;
use axum::extract::ws::WebSocket as AxumWebSocket;
use axum::response::Response;
use clap::builder::ValueParserFactory;
use futures::future::BoxFuture;
use futures::{Future, FutureExt};
use imbl_value::InternedString;
use tokio::sync::{Mutex as AsyncMutex, broadcast};

#[allow(unused_imports)]
use crate::prelude::*;
use crate::util::future::TimedResource;
use crate::util::net::WebSocket;
use crate::util::{FromStrParser, new_guid};

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize, VisitFields,
)]
#[visit(ts(type = "string"), wire = "rpc_toolkit::ts::Unknown")]
#[visit(opaque, type_attributes(visit::wire))]
pub struct Guid(InternedString);

rpc_toolkit::reflect_ts!(Guid);
impl Guid {
    pub fn new() -> Self {
        Self(new_guid())
    }

    pub fn from(r: &str) -> Option<Guid> {
        if r.len() != 32 {
            return None;
        }
        for c in r.chars() {
            if !(c >= 'A' && c <= 'Z' || c >= '2' && c <= '7') {
                return None;
            }
        }
        Some(Guid(InternedString::intern(r)))
    }
}
impl Default for Guid {
    fn default() -> Self {
        Self::new()
    }
}
impl AsRef<str> for Guid {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}
impl FromStr for Guid {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from(s).ok_or_else(|| Error::new(eyre!("invalid guid"), ErrorKind::Deserialization))
    }
}
impl ValueParserFactory for Guid {
    type Parser = FromStrParser<Self>;
    fn value_parser() -> Self::Parser {
        Self::Parser::new()
    }
}

#[test]
fn parse_guid() {
    println!("{:?}", Guid::from(&format!("{}", Guid::new())))
}

impl std::fmt::Display for Guid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

pub struct RestFuture {
    kill: Option<broadcast::Receiver<()>>,
    fut: BoxFuture<'static, Result<Response, Error>>,
}
impl Future for RestFuture {
    type Output = Result<Response, Error>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.kill.as_ref().map_or(false, |k| !k.is_empty()) {
            Poll::Ready(Err(Error::new(
                eyre!("session killed"),
                ErrorKind::Authorization,
            )))
        } else {
            self.fut.poll_unpin(cx)
        }
    }
}
pub type RestHandler = Box<dyn FnOnce(Request) -> RestFuture + Send>;

pub struct WebSocketFuture {
    kill: Option<broadcast::Receiver<()>>,
    fut: BoxFuture<'static, ()>,
}
impl Future for WebSocketFuture {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.kill.as_ref().map_or(false, |k| !k.is_empty()) {
            Poll::Ready(())
        } else {
            self.fut.poll_unpin(cx)
        }
    }
}
pub type WebSocketHandler = Box<dyn FnOnce(AxumWebSocket) -> WebSocketFuture + Send>;

pub enum RpcContinuation {
    Rest(TimedResource<RestHandler>),
    WebSocket(TimedResource<WebSocketHandler>),
}
impl RpcContinuation {
    pub fn rest<F, Fut>(handler: F, timeout: Duration) -> Self
    where
        F: FnOnce(Request) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Response, Error>> + Send + 'static,
    {
        RpcContinuation::Rest(TimedResource::new(
            Box::new(|req| RestFuture {
                kill: None,
                fut: handler(req).boxed(),
            }),
            timeout,
        ))
    }
    pub fn ws<F, Fut>(handler: F, timeout: Duration) -> Self
    where
        F: FnOnce(WebSocket) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        RpcContinuation::WebSocket(TimedResource::new(
            Box::new(|ws| WebSocketFuture {
                kill: None,
                fut: handler(ws.into()).boxed(),
            }),
            timeout,
        ))
    }
    pub fn rest_authed<Ctx, T, F, Fut>(ctx: Ctx, session: T, handler: F, timeout: Duration) -> Self
    where
        Ctx: AsRef<OpenAuthedContinuations<T>>,
        T: Eq + Ord,
        F: FnOnce(Request) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Response, Error>> + Send + 'static,
    {
        let kill = Some(ctx.as_ref().subscribe_to_kill(session));
        RpcContinuation::Rest(TimedResource::new(
            Box::new(|req| RestFuture {
                kill,
                fut: handler(req).boxed(),
            }),
            timeout,
        ))
    }
    pub fn ws_authed<Ctx, T, F, Fut>(ctx: Ctx, session: T, handler: F, timeout: Duration) -> Self
    where
        Ctx: AsRef<OpenAuthedContinuations<T>>,
        T: Eq + Ord,
        F: FnOnce(WebSocket) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let kill = Some(ctx.as_ref().subscribe_to_kill(session));
        RpcContinuation::WebSocket(TimedResource::new(
            Box::new(|ws| WebSocketFuture {
                kill,
                fut: handler(ws.into()).boxed(),
            }),
            timeout,
        ))
    }
    pub fn is_timed_out(&self) -> bool {
        match self {
            RpcContinuation::Rest(a) => a.is_timed_out(),
            RpcContinuation::WebSocket(a) => a.is_timed_out(),
        }
    }
}

pub struct RpcContinuations(AsyncMutex<BTreeMap<Guid, RpcContinuation>>);
impl RpcContinuations {
    pub fn new() -> Self {
        RpcContinuations(AsyncMutex::new(BTreeMap::new()))
    }

    #[instrument(skip_all)]
    pub async fn clean(&self) {
        let mut continuations = self.0.lock().await;
        let mut to_remove = Vec::new();
        for (guid, cont) in &*continuations {
            if cont.is_timed_out() {
                to_remove.push(guid.clone());
            }
        }
        for guid in to_remove {
            continuations.remove(&guid);
        }
    }

    #[instrument(skip_all)]
    pub async fn add(&self, guid: Guid, handler: RpcContinuation) {
        self.clean().await;
        self.0.lock().await.insert(guid, handler);
    }

    pub async fn get_ws_handler(&self, guid: &Guid) -> Option<WebSocketHandler> {
        let mut continuations = self.0.lock().await;
        if !matches!(continuations.get(guid), Some(RpcContinuation::WebSocket(_))) {
            return None;
        }
        let Some(RpcContinuation::WebSocket(x)) = continuations.remove(guid) else {
            return None;
        };
        x.get().await
    }

    pub async fn get_rest_handler(&self, guid: &Guid) -> Option<RestHandler> {
        let mut continuations: tokio::sync::MutexGuard<'_, BTreeMap<Guid, RpcContinuation>> =
            self.0.lock().await;
        if !matches!(continuations.get(guid), Some(RpcContinuation::Rest(_))) {
            return None;
        }
        let Some(RpcContinuation::Rest(x)) = continuations.remove(guid) else {
            return None;
        };
        x.get().await
    }
}

pub struct OpenAuthedContinuations<Key: Eq + Ord>(SyncMutex<BTreeMap<Key, broadcast::Sender<()>>>);
impl<T> OpenAuthedContinuations<T>
where
    T: Eq + Ord,
{
    pub fn new() -> Self {
        Self(SyncMutex::new(BTreeMap::new()))
    }
    pub fn kill(&self, session: &T) {
        if let Some(channel) = self.0.lock().unwrap().remove(session) {
            channel.send(()).ok();
        }
    }
    fn subscribe_to_kill(&self, session: T) -> broadcast::Receiver<()> {
        let mut map = self.0.lock().unwrap();
        if let Some(send) = map.get(&session) {
            send.subscribe()
        } else {
            let (send, recv) = broadcast::channel(1);
            map.insert(session, send);
            recv
        }
    }
}
